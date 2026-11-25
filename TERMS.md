# Terms of Use

_Last updated: 25 November 2026_

By downloading, installing or using Sideport, formerly called Location Spoofer (the standalone app or the browser version, together "the app"), you agree to these terms. If you do not agree, do not use the app.

## 1. What the app is

The app is free software, made by YK3D, for working with an iPhone or iPad you own or are allowed to use, through Apple's developer features. It can:

* simulate the device's GPS location;
* show the device's screen on your computer ("screen mirroring"), play the device's sound, and let you control the device with your mouse and keyboard;
* save screenshots and screen recordings;
* let other programs on your computer use these features through a local programming interface (the "API"), if you turn it on.

It is intended for app development and testing, demonstrations, education, accessibility and personal privacy.

## 2. Acceptable use

You are solely responsible for how you use the app. You must not use it to:

* break any law or regulation that applies to you;
* commit fraud, for example to fake attendance, deliveries, mileage, working hours or presence at a location;
* stalk, harass, deceive or endanger other people, or interfere with emergency services or safety systems;
* access, view, record or control a device without the permission of its owner and its user;
* record calls, video chats, voice messages or other people's conversations, messages or images without the consent the law requires. Recording laws differ between countries and states, and some require every person in a conversation to agree;
* copy or share content you have no right to, such as films, music or paid content. Some apps hide their content from screen mirroring, and the app does not try to get around that;
* breach the terms of service of other apps, games or services. Many games, dating, banking, school, work and delivery apps forbid location simulation or automated control, and may suspend or close accounts that use them.

The app cannot hide that a location is simulated: apps can detect it, and other signals (Wi-Fi, IP address, motion sensors) are unaffected.

## 3. Screen mirroring, recordings and remote control

* Everything on the device's screen and everything it plays can appear on your computer while mirroring is on, including notifications, messages and passwords you type. Turn mirroring off, or keep sensitive apps closed, when other people can see your computer.
* Recordings and screenshots are saved on your computer, where you choose. You are responsible for storing, sharing and deleting them, and for the privacy of anyone who appears in them.
* Clicks, key presses and button presses sent by the app act on the real device exactly like your own touches. Check what you are doing before you tap: the app cannot undo a purchase, a deleted item or a sent message.
* Screen mirroring uses an Apple developer service that only recent iOS versions provide. It may stop working after an iOS update.

## 4. The API for other programs

The API is off until you turn it on in *Settings › Developer API*. When it is on, any program on your computer that has the API key can do what the app can do: read the device's screen, control the device and change its location. Only give the key to programs you trust, and create a new key (which stops the old one working) if you no longer trust one. Programs that use the API are not made by YK3D, and their own terms and privacy policies apply to them.

## 5. No warranty

THE APP IS PROVIDED "AS IS" AND "AS AVAILABLE", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING FITNESS FOR A PARTICULAR PURPOSE, ACCURACY AND NON-INFRINGEMENT. Routes, speeds, travel times and speed limits are estimates and may be wrong. Mirrored pictures and sound can lag, freeze or be incomplete. **Never use the app for navigation or safety-critical purposes.**

## 6. Limitation of liability

To the maximum extent permitted by law, the authors and contributors are not liable for any damages or losses arising from the use of, or inability to use, the app. This includes account suspensions, data loss, actions taken on a device through the app or its API, device problems, and charges from third-party services such as Google Maps Platform. Nothing in these terms limits liability that cannot be limited by law.

## 7. Your devices

The app needs Developer Mode on the device and communicates with Apple's developer services. You use these features at your own risk. Turn off location simulation ("Stop spoofing", or restart the device) and screen mirroring when you are done.

## 8. Third-party services and keys

Maps, place search, routing, speed limits, updates and Developer Disk Images come from third parties (see [PRIVACY.md](PRIVACY.md)). Your use of them is subject to their terms, for example the [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/), the [Nominatim usage policy](https://operations.osmfoundation.org/policies/nominatim/) and the [Google Maps Platform Terms of Service](https://cloud.google.com/maps-platform/terms).

If you add a Google Maps API key:

* it is your own key;
* you are responsible for its billing and its restrictions;
* you must follow Google's terms. The app follows them by showing Google routes only on Google's map and by not exporting Google route data.

The public OpenStreetMap services are run by volunteers; please do not use the app for heavy automated requests.

## 9. Trademarks and affiliation

The app is an independent project. It is **not affiliated with, endorsed by or sponsored by Apple Inc. or Google LLC**. iPhone, iPad, iOS and Xcode are trademarks of Apple Inc. Google Maps is a trademark of Google LLC. Other names belong to their respective owners and are used only to describe compatibility.

## 10. Ownership and licences

The app, its source code, interface, icon and documentation are owned by YK3D and protected by copyright. You may use the app for free, but you may not copy its code or artwork into other projects, redistribute it, sell it or publish modified versions without written permission, and any permitted use must credit YK3D in the way the licences describe. The exact rules are in the licences:

* the standalone app, the shared interface, the icon and the documentation: the Location Spoofer License ([LICENSE](LICENSE)), which keeps the app's former name;
* the browser version: the GNU GPL v3 or later with additional terms that require the same credit ([browser/LICENSE](browser/LICENSE), [browser/NOTICE](browser/NOTICE)), because it is distributed together with GPL-licensed libraries.

Those licences govern copying, modifying and redistributing the code. These terms govern using the app.

## 11. Updates and changes

The app can update itself from this repository's GitHub releases. Releases are checked with a SHA-256 checksum but are not code-signed. These terms may change; the version in this repository applies from its "Last updated" date.

## 12. Contact

Report problems, or ask for permission to use the code, at <https://github.com/YK3D/iPhone-location-spoofer/issues>. Report security issues as described in [SECURITY.md](SECURITY.md).
