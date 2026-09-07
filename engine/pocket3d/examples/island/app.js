// Hot-reloadable application behavior. The native host owns rendering,
// animation, collision and input sampling; this object has no SDK bindings.
globalThis.islandApp = {
  version: 1,
  title: "A little island, together.",
  room: "LOCAL ROOM",
  camera: { span: 7.2, eyeHeight: 6.6, distance: 10, targetHeight: 0.8, yaw: 0, tilt: 0 },
  phrases: [
    "Hello, island!",
    "Let's take a walk.",
    "This is my happy place.",
    "See you by the sea!",
  ],
  handle(event) {
    switch (event.type) {
      case "wave": return { flags: 2 };
      case "sit": return { flags: 4 };
      case "cheer": return { flags: 8 };
      case "previousExpression": return { expression: (event.expression + 6) % 7 };
      case "nextExpression": return { expression: (event.expression + 1) % 7 };
      case "expression": return { expression: event.value };
      case "phrase": return { message: this.phrases[event.value] };
      case "message": return { message: event.text.trim() };
      default: return {};
    }
  },
};
