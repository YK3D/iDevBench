---
name: sideport-iphone
description: Operate a real iPhone through the Sideport MCP tools (screenshot, tap, swipe, type, gestures, buttons, GPS location). Use when asked to test, use, explore or automate an iOS app or setting on a connected iPhone, or to check how an app behaves at another location.
---

# Using an iPhone through Sideport

The `sideport` MCP server controls a real iPhone plugged into this computer, through the Sideport app (which must be open). You see the phone with `screenshot` and act with `tap`, `swipe`, `type_text` and friends.

## Start

1. `iphone_status`. If `screen.state` is not `on`, call `start_screen`. It fails if the phone is locked, has no Developer Mode, runs iOS older than 27, or the Camera or a recording app is open: tell the user what the error says and stop.
2. `screenshot` to see where you are.

## The loop

Look, act once, look again. Never chain several taps blind: screens change, keyboards appear, alerts pop up.

- `screenshot`, decide on one action, do it, then `screenshot` again (use `wait_ms` 500–2000 after opening an app, navigating or submitting a form).
- If nothing changed, the tap missed or the app is still busy. Wait once, then retry with an adjusted position. Don't repeat the same miss more than twice.

## Positions

Every position is a fraction of the screenshot: `x` from 0 (left) to 1 (right), `y` from 0 (top) to 1 (bottom). Work it out from the screenshot: `x = pixel x / width`, `y = pixel y / height`. Aim for the centre of a button, not its edge.

Avoid the very edges unless you mean a system gesture: the top ~5% is the status bar, the bottom ~3% is the home indicator.

## Actions

| Want | Tool |
|---|---|
| Press a button, link, field, cell | `tap` |
| Scroll down a list | `swipe` from `y: 0.75` to `y: 0.3` (the finger moves up). Scroll up: the reverse |
| Swipe between pages / delete a row | `swipe` sideways |
| Context menu, rearrange icons | `long_press` |
| Type into a field | `tap` the field, check the keyboard is up (`screenshot`), then `type_text`. US layout; characters it can't type (accents, emoji) are skipped |
| Submit / new line | `press_key` `return` |
| Select all / copy / paste | `press_key` `a` / `c` / `v` with `command: true` |
| Go to the Home Screen | `gesture` `home` (or `press_button` `home` on phones with a Home button) |
| Switch apps | `gesture` `app_switcher` |
| Go back | the app's back button first; `gesture` `back` otherwise |
| Open an app | Home Screen, then `swipe` down from the middle to open Search, `type_text` its name, tap the result |
| Pretend to be somewhere | `connect_device`, then `set_location` with latitude and longitude; `clear_location` when done |

## Care

- This is someone's real phone. Don't buy anything, send messages, post, delete data, change passwords or accept terms unless the user asked for exactly that; ask first when an action can't be undone.
- Screenshots can show private information (messages, notifications, emails). Use only what the task needs and don't repeat the rest.
- If a system alert asks for a permission (location, notifications, tracking), choose what the user's task needs; ask when unsure.
- When finished, leave the phone where the user would expect it (usually the app you were testing), and say what you did and what you saw.

## When it goes wrong

- `screenshot` fails with "not running": call `start_screen` again.
- "Sideport is not running": ask the user to open the Sideport app.
- The phone locked itself: ask the user to unlock it (you can't type the passcode).
