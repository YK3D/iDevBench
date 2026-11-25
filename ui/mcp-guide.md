# Using an iPhone through iDevBench

The `idevbench` tools control a real iPhone connected to this computer, through the iDevBench app (which must be open). The phone belongs to the user: act carefully.

## Location

- `connect_device` once (`driver: "mock"` tries things without a phone).
- Find coordinates with `search_place` (or `parse_map_link` for a Google Maps link), then `set_location`.
- Movement: `drive_route` with checkpoints and one leg per stretch (`car`, `bike`, `foot`, `sea` or `straight`); `play_track` for a list of points; `joystick` to move live. Control playback with `playback` (pause, resume, continue, stop) and `playback_options`; check progress with `get_status`.
- `clear_location` when done, unless the user wants the phone to stay there.

## Screen (iOS 27 and newer)

1. `mirroring_status`; if it isn't `on`, `start_mirroring`. If that fails (locked phone, no Developer Mode, older iOS, Camera or a recording app open), tell the user what the error says.
2. Work in a loop: `screenshot`, decide one action, do it, `screenshot` again. Use `wait` (500-2000 ms) after opening an app, navigating or submitting. Never chain several taps blind.
3. If nothing changed, the tap missed or the app is busy: wait once, then retry with an adjusted position. Don't repeat the same miss more than twice. If screenshots stop changing at all, `refresh_mirroring`.

Positions are fractions of the screenshot: `x = pixel x / width`, `y = pixel y / height` (0,0 top left). Aim for the centre of a control. Avoid the top ~5% (status bar) and bottom ~3% (home indicator) unless you mean a system gesture.

| Want | Tool |
|---|---|
| Press a button, link, field or cell | `tap` |
| Scroll down | `swipe` from `y: 0.75` to `y: 0.3` (the finger moves up); up is the reverse |
| Context menu, move icons | `long_press` |
| Type | `tap` the field, check the keyboard is up, then `type_text` (US layout; accents and emoji are skipped) |
| Submit | `press_key` usage 40 (Return) |
| Select all / copy / paste | `press_key` usage 4 / 6 / 25 with `command: true` |
| Home Screen, app switcher, Control Centre, notifications, back | `gesture` |
| Open an app | `gesture` home, `swipe` down from the middle to open Search, `type_text` its name, `tap` the result |
| Volume, lock, Siri | `press_button` |
| How hard the phone works | `phone_performance` (ask twice; the first call starts measuring) |

## The passcode

If the phone is locked, ask the user to unlock it. Unlock it yourself only when the user asks you to and gives you the passcode in this conversation:

1. `press_button` lock to wake the screen if it is dark, then `gesture` home (swipe up) to show the keypad, and `screenshot`.
2. `type_text` the passcode (if nothing appears, `tap` the keypad digits instead), then `screenshot` to check it opened.
3. Use the passcode only to unlock. Never write it into files, notes, logs or other apps, and don't repeat it back.
4. Never guess a passcode, and stop after one failed attempt: wrong passcodes lock the iPhone for longer and longer and can erase it.

## Care

- Don't buy anything, send messages, post, delete data, change passwords or settings, or accept terms unless the user asked for exactly that; ask first when an action can't be undone.
- Screenshots can show private information (messages, notifications). Use only what the task needs and don't repeat the rest.
- Permission alerts (location, notifications, tracking): choose what the task needs; ask when unsure.
- When finished, say what you did and what you saw.
