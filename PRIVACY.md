# Privacy Policy

_Last updated: 25 November 2026_

This policy covers both editions of iDevBench, formerly called Location Spoofer: the standalone app (`LocationSpooferStandalone.exe`) and the browser version (`LocationSpooferBrowser.exe` or run from source). Together they are called "the app".

## The short version

* The app has **no accounts, no analytics, no advertising and no telemetry**. Its authors receive **no data** from you.
* Everything you create — favourites, saved routes, settings, an optional Google API key — is stored **only on your computer**.
* To show maps, find places and calculate routes, the app talks **directly from your computer to third-party online services** listed below. Those services receive the information needed to answer each request (for example map coordinates or a search term) and your IP address, under their own privacy policies.
* The app talks to your iPhone **only over the USB cable or your local network**; nothing about the phone is sent to the internet by the app.
* **Screen mirroring** (the picture, the sound and your clicks and key presses) travels only between the phone and your computer. The app does not upload it, analyse it or keep a copy. Only screenshots and recordings that you save are written to disk, where you choose.
* The **local API for other programs** works only while the app is open, only for programs on your computer (not websites or other computers). Programs that use it can receive the phone's screen and control the phone; what they do with it is covered by their own privacy policies.

## What is stored on your computer

| Data | Where | Why |
|---|---|---|
| Favourite places, saved routes, settings (theme, shortcuts, panel sizes, last map position), the update channel, the tutorial status | Standalone: `%APPDATA%\LocationSpooferStandalone\data.json` (Windows). Browser version: `%APPDATA%\iPhoneLocationSpoofer\data.json`. On macOS/Linux, under your user data folder. | So the app remembers your setup |
| Google Maps API key (only if you enter one) | The same `data.json` file | So the app can call Google on your behalf. It is never included in backup exports |
| iPhone pairing record | Managed by Apple's device service (or the built-in service in experimental builds) | Needed for the iPhone to trust this computer |
| Apple Developer Disk Image cache | `ddi` folder next to `data.json` | Avoids downloading it again |
| Offline maps: downloaded map areas, and the map styles and fonts | `offline` folder next to `data.json` | So the map works without internet. Delete areas in Settings › Offline maps |
| Developer access only: the remembered developer password and downloaded feature-test builds | `feature-tests` folder next to `data.json` | Only if the developer unlocks feature tests and ticks "Remember" |
| Downloaded updates | Next to the app's `.exe` until installed | Self-update |
| Screenshots and screen recordings (only the ones you save) | The folder you pick when saving (usually *Downloads*) | So you can keep them. The app does not keep its own copy |

You can delete all of it at any time by deleting those folders. Uninstalling the app (deleting the `.exe`) does not delete `data.json`.

While screen mirroring is on, the most recent pictures and sound from the phone are held briefly in the computer's memory so they can be shown and played. They are gone when you stop mirroring or close the app.

## Online services the app contacts

The app sends requests **directly** to these services. No data passes through servers run by the app's authors.

| Service | When | What it receives | Policy |
|---|---|---|---|
| OpenFreeMap (`tiles.openfreemap.org`) | Showing the OpenFreeMap map (the default), and downloading offline map areas | Map tile coordinates, IP address, the app's name and version | [OpenFreeMap terms](https://openfreemap.org/tos/) |
| OpenStreetMap tile servers (`tile.openstreetmap.org`) | Showing the OpenStreetMap map | Map tile coordinates, IP address | [OSMF privacy policy](https://osmfoundation.org/wiki/Privacy_Policy) |
| Esri World Imagery | Showing the Satellite map | Map tile coordinates, IP address | [Esri privacy](https://www.esri.com/en-us/privacy/overview) |
| OpenTopoMap | Showing the Topographic map | Map tile coordinates, IP address | [OpenTopoMap](https://opentopomap.org/about) |
| Nominatim (`nominatim.openstreetmap.org`) | Place search; Google Maps links that contain place names | Your search text, IP address | [OSMF privacy policy](https://osmfoundation.org/wiki/Privacy_Policy) |
| FOSSGIS OSRM routing (`routing.openstreetmap.de`) | Building road, cycle or footpath routes | Start and end coordinates of each leg, IP address | [FOSSGIS privacy policy](https://www.fossgis.de/datenschutzerkl%C3%A4rung), [service info](https://routing.openstreetmap.de/about.html) |
| Overpass API (`overpass-api.de`) | Only when "Use public speed limits" is turned on | OpenStreetMap node IDs along the route, IP address | [Overpass API](https://wiki.openstreetmap.org/wiki/Overpass_API) |
| Google Maps Platform (Routes API, Map Tiles API) | Only if you add your own API key and choose Google routing | Route coordinates, map tile requests, your API key, IP address | [Google Privacy Policy](https://policies.google.com/privacy) |
| Google short links (`maps.app.goo.gl`) | Only when you paste a short Google Maps link | The link, IP address | [Google Privacy Policy](https://policies.google.com/privacy) |
| GitHub (`api.github.com`, `github.com`, `release-assets.githubusercontent.com`) | Update checks (at start-up and every few hours) and update downloads; for the developer only, encrypted feature-test builds after unlocking *Developer access* | Your IP address and app version (in the request) | [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) |
| GitHub raw content (`raw.githubusercontent.com/doronz88/DeveloperDiskImage`) | Only when the iPhone has no Developer Disk Image mounted | The requested file path, IP address | [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) |

Screen mirroring, screenshots, recordings, the API, straight-line and sea-lane routes, coordinate entry, GPX files, the simulator and downloaded offline map areas work **offline** and contact none of these services (the Developer Disk Image may be downloaded once, as listed above). Update checks can be turned off by starting the app with `--no-update`.

## Your real location

When you press **Show my real location** or **Stop spoofing**, the app asks your computer's operating system for its approximate position. That request is handled by Windows (or your browser) and may involve its own location services; you can deny the permission. The position is only drawn on the map and is not stored or sent anywhere by the app.

## Other people on your screen

Mirrored pictures, sound and recordings can show other people's messages, photos, names and voices. You decide whether to save or share them, and the [Terms of Use](TERMS.md) ask you to respect the law and other people's privacy when you do.

## Children

The app is not directed at children and collects no personal information from anyone.

## Changes

Changes to this policy are published in this repository with a new "Last updated" date. Significant changes are mentioned in the release notes.

## Contact

Questions about privacy can be raised as an issue on the project's GitHub page: <https://github.com/YK3D/iPhone-location-spoofer/issues>.
