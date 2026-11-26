# Changelog

Releases are published separately for the two editions:
[standalone app](https://github.com/YK3D/iDevBench/releases?q=standalone&expanded=true) (`standalone-v…`) and
[browser edition](https://github.com/YK3D/iDevBench/releases?q=browser&expanded=true) (`browser-v…`).
Test builds from the `dev` branch are marked as pre-releases.

## Unreleased (dev)

### Experimental (see [docs/testing/multiple-iphones-and-wifi.md](docs/testing/multiple-iphones-and-wifi.md))
- Several iPhones at once: phones added under *Device › More iPhones* follow the main one (teleports, routes, GPX, joystick, Stop spoofing). Each gets positions from its own queue, so a slow or lost phone never holds up the others; a lost phone is marked until removed, and removing one sends it back to its real location. The Simulator has a second phone to try it.
- Wi-Fi connections: *Device tools › Connect over* (Automatic, USB cable only, Wi-Fi only) and *Allow Wi-Fi connection*, which turns on the phone's "Show this iPhone when on Wi-Fi" setting. The device list shows how each phone can be reached (USB, Wi-Fi, or both). In the browser version, a second iOS 17+ phone or an iOS 17+ phone over Wi-Fi needs the tunnel helper. Bluetooth isn't possible.

### New
- **Open source:** iDevBench is now under the MIT License (the browser edition stays GPL v3 or later). The repository is now [YK3D/iDevBench](https://github.com/YK3D/iDevBench).
- **New file names:** the downloads are now `iDevBench.exe` and `iDevBenchBrowser.exe`. Copies installed under the old names keep updating.
- The setup guide has a step for screen mirroring.
- Fixed: the default map stayed blank while OpenFreeMap serves its backup map.
- Fixed: in the Mirroring section, a hidden side menu could not be shown again. The phone shown before the picture arrives is now in shades of grey, with its side buttons where an iPhone has them.
- **Screen mirroring (iOS 27 and newer):** the iPhone's screen in its own *Mirroring* section (switch at the top of the side menu), over the USB cable. Click to tap, drag to swipe, scroll with the mouse wheel, Ctrl + wheel or a trackpad pinch to zoom, right-click for Home, and type from your keyboard (Ctrl works as ⌘). Buttons next to the screen press Home, Lock, Volume, Mute and Siri and do the app switcher, Control Centre, notifications and Back gestures. Over the screen the pointer is a small dot that dips when you click, and swipes leave a faint trail. The phone's name and iOS version are shown above it, and an iPhone home screen stands in until the picture arrives. The picture turns when the phone does (or with *Rotate*). The standalone app also plays the iPhone's sound. Save screenshots, or record the screen with its sound as a video, to Downloads or a folder chosen with *Browse* in *Settings › Mirroring*. *Stats* shows the stream's frame rate, data rate and delay, and the iPhone's CPU, memory, GPU, battery and temperature. A frozen picture restarts by itself after a few seconds, or at once with *Restart stream*.
- **Local API:** other programs on your computer can set the location, watch the screen, tap, swipe and type through the app while it is open (websites can't). See [docs/API.md](docs/API.md).
- **AI agents:** iDevBench is an MCP server, so Claude, Codex, Cursor and other AI apps can use the phone through it: see and control the screen, set the location, drive routes, play tracks and use the joystick. Setting it up is one prompt to paste into the agent (*Settings › AI agents*), or one button for Claude Desktop; there is also a Claude Code plugin. The agent gets a guide on how to use the phone, and only enters the passcode if you ask it to and give it the passcode. See [docs/API.md](docs/API.md#ai-agents-mcp).
- **iOS 27:** the standalone app now tries to mount the developer image itself instead of asking for Xcode first.
- **New name: iDevBench.** Location Spoofer is now called iDevBench, because it is growing into a toolkit for working with your iPhone from Windows, not only for its location. Nothing else changes: updating keeps your settings, favourites, routes and offline maps, and the download files keep their names so installed copies keep updating.
- **Website:** [idevbench.vercel.app](https://idevbench.vercel.app) shows what the app does, has the downloads, and lets you try a small version of it in your browser. The Terms of Use, Privacy Policy and licences now open there (setup guide and *Settings › About*).
- **Offline maps:** *Settings › Offline maps* saves the area on screen (up to 3,000 map tiles, about a city, per download; as many areas as you like), so the map works without internet. Saved areas can be shown on the map or deleted.
- **New map menu** (top right): picture previews of each map, the map theme (Dark, Light or Auto) and a shortcut to offline maps.
- **Smooth trackpad zoom:** pinching or two-finger scrolling zooms smoothly to exactly where your fingers stop, instead of jumping in steps and flickering.
- Buttons sink in slightly when pressed, and a wave spreads from the point you pressed.
- **Feature tests (developer only):** developer mode is hidden until you click the *About* tab in Settings 7 times; then *Settings › Updates › Developer access* opens encrypted builds of unreleased experiments with the developer password. After unlocking, every build downloads in the background, so *Open* starts it at once as a separate app in its own window, with its own settings, and it doesn't update itself.
- The setup guide and the README link straight to **Apple Devices** in the Microsoft Store.
- **A real dark map:** the default map (OpenFreeMap) is sharp at every zoom and has a true dark style and a light style. Choose the map in *Settings › Appearance › Map* or with the map's layer button. If it can't load, the app says so and shows OpenStreetMap.
- **Realistic cornering:** every bend is taken at the speed its curve allows for the vehicle (cars, bikes, walkers and boats differ). For a car: a right-angle junction at about 18 km/h, a roundabout at about 22 km/h, a gentle motorway curve at about 88 km/h. *Slow down for turns and U-turns* is on by default, and *Route summary* shows the sharpest bend.
- **Roundabouts** are always driven at a normal roundabout speed for their size (about 20–40 km/h by car), whatever the road's own speed.
- **Edit routes on the map:** drag the route line to add a checkpoint there, double-click a checkpoint to delete it, or right-click one for Delete, Insert after, Set wait and Make start.
- **Hold until Continue:** a checkpoint can stop playback until you press Continue (or **N**).
- **Alternative routes:** ask for up to three in *Routing & speed*, then pick one in *Route summary* or click a grey line on the map. Each is labelled with its extra time.
- **Build progress:** the Build route button shows each step and how far along it is. Cancel it with the Cancel button or Esc; changing the route cancels it by itself.
- **Laps** for looping routes, and live **speed and heading** on the map while playing.
- Drag checkpoints by their handle to reorder them, and hover one in the list to highlight it on the map.
- **Looks:** 17 colour themes (Graphite by default), a dark or light map, a compact layout, folding sections, a side menu you can resize or hide (**M**), animated dialogs, lists and a setup guide with small animations.
- **Keyboard shortcuts** for common actions, changeable in *Settings › Shortcuts* (press **H** for the list).
- **Settings** window with Appearance, Shortcuts, Google Maps, Updates and About pages.
- **Update channels:** stable releases, or opt-in test builds.
- *Stop spoofing* and *Show my real location* show your computer's real location on the map.
- Undo after clearing checkpoints or deleting favourites and saved routes.
- A new app icon. The taskbar shows just the red location pin, so it stays clear at small sizes.

### Fixed
- Choosing a map or map theme in the map menu also picked a point on the map underneath it (or added a checkpoint on the Route tab).
- After an update, Windows kept showing the old icon on pinned taskbar buttons and shortcuts until the app was downloaded again. The app now asks Windows to refresh its icons when it installs an update and when the new version first starts.
- With a checkpoint's right-click menu open, a left click on the map added a checkpoint. It now only closes the menu.
- Dropdowns in Settings opened behind the Settings window.
- Roundabouts were sometimes driven at 70 km/h or more.
- Changing the default speed didn't mark a built route as out of date.
- Asking for three alternative routes often gave only one or two.
- Clicking a checkpoint marker added another checkpoint.
- The heading jumped at each bend of the road instead of turning smoothly.
- The standalone window flashed white at start-up.
- Markers were drawn slightly off their point; the speed-coloured route line didn't pass exactly through every checkpoint.
- Public speed limits could fail for a whole route, or no longer line up with it.
- After clearing a route or removing a GPX file, the player still showed the old one.
- Opening a new version could show an older copy that was still running in the background.
- Other websites open in your browser can no longer send commands to the app or read its settings.

### Changed
- **Licence:** Location Spoofer is still free to use, but its code, interface and icon may no longer be copied, redistributed or modified without written permission, and any permitted use must credit YK3D (see LICENSE). The browser edition stays under the GNU GPL, now with an attribution requirement.
- A cleaner, quieter interface: sections are cards with an icon, tabs are a segmented control, help icons explain every tool, and the Route tab starts with every section folded.
- The route line is coloured by speed in a smooth gradient and outlined so it stands out from the roads.
- "Use public speed limits" is marked as not recommended (the public service is slow and often incomplete).
- Both editions share one interface. The standalone app is the recommended edition; releases are named `standalone-v…` and `browser-v…`.

## standalone-v0.1.1 and v1.2.2 — 1 November 2026
- First standalone (Rust) app with a native window.
- Google Maps links and optional Google routing, experimental road-aware driving, speed-coloured routes, setup guide and info buttons.
- Rebuilt GPX panel and many fixes.

## v1.1.1 — 1 November 2026
- First Windows `.exe` with automatic updates.
