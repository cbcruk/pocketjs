//! The HTTP transport `pocket-net` leaves to the host.
//!
//! `NetCore` owns handles, limits and the tick-boundary batching; everything
//! here is the part it deliberately does not: DNS, sockets, TLS and a thread
//! to block on them. The contract is that `start` returns promptly and
//! `drain` never blocks, so requests run on a worker and their results cross
//! back over a channel.
//!
//! One worker, not a pool. The panel presents about twice a second and the
//! device has 256 MB; a screen that wants six fetches at once wants them for
//! one repaint, and serializing them costs it nothing it can display.
//!
//! TLS lives in this binary because it cannot live on the device. This
//! firmware ships OpenSSL 0.9.8l from 2009 — no TLS 1.2, no SNI — and its
//! busybox wget refuses an `https://` URL outright. A host that wanted to
//! shell out for HTTPS had nothing to shell out to.

use std::collections::{BTreeMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use pocket_net::{HttpRequest, HttpTransport, NetFailure, TransportCompletion};
use pocketjs_core::spec::net as spec;

/// Bytes read from a response body per pull. Small enough that a runaway
/// server is noticed near the limit rather than a chunk past it.
const READ_CHUNK: usize = 16 * 1024;

pub struct UreqTransport {
    jobs: Sender<HttpRequest>,
    results: Receiver<TransportCompletion>,
    /// Handles the guest gave up on. A request already in flight cannot be
    /// interrupted — its timeout bounds it — so the answer is dropped when it
    /// arrives rather than pretended away.
    cancelled: HashSet<i32>,
}

impl UreqTransport {
    pub fn new() -> Self {
        let (jobs, incoming) = channel::<HttpRequest>();
        let (outgoing, results) = channel::<TransportCompletion>();
        std::thread::Builder::new()
            .name("pocketjs-net".into())
            .spawn(move || {
                // Ends when the host drops its sender, which is process exit.
                for request in incoming {
                    let completion = perform(request);
                    if outgoing.send(completion).is_err() {
                        break;
                    }
                }
            })
            .expect("spawning the network worker");
        Self {
            jobs,
            results,
            cancelled: HashSet::new(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn start(&mut self, request: HttpRequest) -> Result<(), NetFailure> {
        self.cancelled.remove(&request.handle);
        self.jobs.send(request).map_err(|_| {
            NetFailure::new(spec::ERROR_UNAVAILABLE, "the network worker is gone")
        })
    }

    fn cancel(&mut self, handle: i32) {
        self.cancelled.insert(handle);
    }

    fn drain(&mut self, completions: &mut Vec<TransportCompletion>) {
        loop {
            match self.results.try_recv() {
                Ok(completion) => {
                    let handle = match &completion {
                        TransportCompletion::Done { handle, .. }
                        | TransportCompletion::Error { handle, .. } => *handle,
                    };
                    if self.cancelled.remove(&handle) {
                        continue;
                    }
                    completions.push(completion);
                }
                Err(TryRecvError::Empty) => return,
                // The worker died. Say so once rather than spinning: every
                // later start() reports the same thing through its own error.
                Err(TryRecvError::Disconnected) => return,
            }
        }
    }
}

fn perform(request: HttpRequest) -> TransportCompletion {
    let handle = request.handle;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(request.timeout_ms.max(1) as u64))
        .redirects(request.max_redirects as u32)
        .build();

    let mut call = agent.request(&request.method, &request.url);
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }

    let response = match call.send_bytes(&request.body) {
        Ok(response) => response,
        // A status the guest asked about is not a transport failure: 404 is an
        // answer. ureq calls those errors, so unwrap them back into responses.
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(transport)) => {
            return TransportCompletion::Error {
                handle,
                failure: classify(&transport),
            };
        }
    };

    let status = response.status();
    let url = response.get_url().to_string();
    let mut headers = BTreeMap::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            headers.insert(name.to_ascii_lowercase(), value.to_string());
        }
    }

    match read_capped(response, request.max_bytes) {
        Ok(body) => TransportCompletion::Done {
            handle,
            status,
            url,
            headers,
            body,
        },
        Err(failure) => TransportCompletion::Error { handle, failure },
    }
}

/// Reads at most `max_bytes`, and reports the overrun rather than truncating.
///
/// A body silently cut at the limit is worse than no body: the guest parses a
/// prefix and gets a plausible wrong answer.
fn read_capped(response: ureq::Response, max_bytes: usize) -> Result<Vec<u8>, NetFailure> {
    use std::io::Read;

    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| NetFailure::new(spec::ERROR_PROTOCOL, error.to_string()))?;
        if read == 0 {
            return Ok(body);
        }
        if body.len() + read > max_bytes {
            return Err(NetFailure::new(
                spec::ERROR_RESPONSE_TOO_LARGE,
                format!("body exceeds the {max_bytes}-byte limit"),
            ));
        }
        body.extend_from_slice(&chunk[..read]);
    }
}

/// Maps a ureq transport error onto the portable codes in `net.ts`.
///
/// ureq's kinds are close but not identical to the contract's, and the guest
/// only sees these strings — a wrong one sends an app retrying a DNS failure
/// as though the server were merely busy.
fn classify(error: &ureq::Transport) -> NetFailure {
    use ureq::ErrorKind;

    let code = match error.kind() {
        ErrorKind::Dns => spec::ERROR_DNS,
        ErrorKind::ConnectionFailed => spec::ERROR_CONNECT,
        ErrorKind::InvalidUrl | ErrorKind::UnknownScheme | ErrorKind::BadHeader => {
            spec::ERROR_INVALID_REQUEST
        }
        ErrorKind::TooManyRedirects => spec::ERROR_REDIRECT,
        ErrorKind::BadStatus | ErrorKind::HTTP => spec::ERROR_PROTOCOL,
        // This device has no TLS to borrow, so a plain-http request is a
        // choice the app made, not a fallback it stumbled into.
        ErrorKind::InsecureRequestHttpsOnly => spec::ERROR_TLS,
        ErrorKind::InvalidProxyUrl | ErrorKind::ProxyConnect | ErrorKind::ProxyUnauthorized => {
            spec::ERROR_CONNECT
        }
        ErrorKind::Io => {
            // ureq folds a timeout into Io; the message is the only signal it
            // gives, and a timeout the guest reads as a protocol error would
            // send it retrying immediately against a server it just gave up on.
            let message = error.to_string().to_ascii_lowercase();
            if message.contains("timed out") || message.contains("timeout") {
                spec::ERROR_TIMEOUT
            } else {
                spec::ERROR_CONNECT
            }
        }
        // Exhaustive on purpose. A catch-all would map a kind nobody has
        // looked at onto ERROR_OTHER, and the guest would be told nothing
        // useful about something that had a right answer available; failing
        // the build when ureq adds one is the moment to decide.
    };
    NetFailure::new(code, error.to_string())
}
