import Features from "./Features.jsx";

const ITEMS = [
  {
    key: "mirroring",
    title: "See its screen, and use it.",
    clip: "mirroring",
    label: "The Mirroring section: Start mirroring, and the iPhone's home screen appears in the app; then its apps are dragged into folders",
    body: "With iOS 27 or newer, the iPhone's screen appears live in the app over the USB cable, with its sound. Click to tap, drag to swipe and type on your keyboard.",
    specs: [
      "Home, Lock, Volume, Mute and Siri, plus the app switcher, Control Centre, notifications and Back",
      "Scroll with the wheel, Ctrl + wheel to pinch; the picture turns with the phone",
      "Screenshots and recordings with sound, and live stats: frame rate, CPU, memory, GPU and temperature",
    ],
  },
  {
    key: "agents",
    title: "Hand it to an AI agent.",
    clip: "agent",
    label: "Claude Code, connected to iDevBench, is asked to drive from the current location to the airport and check it on the screen: it finds Heathrow, starts a car route, opens Find My on the mirrored iPhone and sees it moving",
    body: "iDevBench has a built-in MCP server, so Claude, Codex, Cursor, Gemini, Copilot, Windsurf and other AI apps can use the phone: see the screen, tap and type, set the location and drive routes.",
    specs: [
      "Set up with one prompt to paste into the agent, or one button for Claude Desktop; Claude Code also has a plugin",
      "A built-in guide teaches the agent to look, act and check, and to ask before anything it can't undo",
      "It enters your passcode only if you ask it to and give it the passcode; screenshots go to the AI service the agent runs on",
    ],
  },
];

// The iPhone section: screen mirroring and AI agents, in the same layout as the location features.
export default function Phone() {
  return (
    <Features
      id="phone"
      idx="05–06"
      kicker="Beyond location"
      title="Mirror it. Or let an AI drive."
      intro="These clips use the app's simulated iPhone, which has no screen of its own, so the phone screen in them is a stand-in drawn for the recording. The app, the clicks and the agent's tool calls are real."
      items={ITEMS}
      first={5}
    />
  );
}
