# iDared 32bit changelog

iDared 32bit is based on [touchHLE](https://touchhle.org/). This file lists only iDared 32bit's own changes. The touchHLE changes it includes are in [CHANGELOG.md](CHANGELOG.md), which is touchHLE's own changelog, kept exactly as it is upstream.

## NEXT

Compatibility:

- Fixed crashes in some games that work with dates.
- Fixed some games freezing during animations.
- Fixed missing music in some games.
- Fixed some games crashing at random.
- Fixed a crash on iPhone and iPad when a game rotates the screen as it starts.
- Fixed a crash when starting some older games.
- Navigation bars in games are now drawn, with working buttons, so some games' settings and store screens no longer crash.

## v1.5.0 (2026-10-23)

Usability:

- When a game opens a web page, it now opens on iPhone and iPad too, and iDared 32bit returns to the app picker instead of closing.
- Toolbars in games look as they did on iPhone OS 3, with glossy bars and buttons, instead of plain white buttons.
- On iPhone and iPad, the app picker's header background can be blue, black, purple, pink, green, orange or teal, and its text yellow, white, gray, black, purple, orange or teal, chosen in Theme Options.
- The app picker's Quick Options button is now Settings: the same game options, plus a Theme Options page.
- Settings can choose whether games made for both iPhone and iPad run their iPhone or iPad version.
- Theme Options can hide the logo in the app picker's header, showing the name instead with a shorter header.
- Theme Options can also hide the names under the games' icons, and give the footer the header's background color.
- On iPhone and iPad, a wallpaper for the app picker can be chosen from Photos, in Theme Options.
- On iPhone and iPad, games added or removed with File Manager now show up in the app picker as soon as you come back to it.
- On iPhone and iPad, games can be imported with the new Import button in the app picker, or shared to iDared 32bit from other apps. The website link is now in Settings.
- On iPhone, iPad and Android, games (`.ipa` files) put in the main folder instead of `Games` are moved into `Games` when the app picker opens, or on iPhone and iPad when you come back to it.

Compatibility:

- Fixed a crash in some games when loading their text.
- Fixed missing music in some games that use 8-bit sound files.
- Fixed a crash in some games that use AIFF sound files.
- Fixed crashes in some games when starting their sound, and missing sound in some games.
- Fixed a crash in some games when deleting text.
- Fixed a crash on iPhone and iPad in some games after playing for a while.
- Alerts in games are now shown, and their buttons work, fixing a crash in some games.
- Fixed a crash in some games that watch for changes to their settings.
- Fixed some games that look through folders of files, such as their saves.
- Games now use the fonts they come with.
- Fixed menus and screens in some games that stopped partway through their animations, didn't show their buttons, or stayed black.
- Animated images in games now play.
- Fixed a black screen at start-up in some games.
- Various small fixes.

Other:

- On iPhone, iPad and Android, the license information now includes the full text of the GNU GPL, which the libgcc and libstdc++ libraries are licensed under.
- The license information now includes the license texts of the Rust libraries and other code that iDared 32bit uses.

## v1.4.3 (2026-10-18)

Usability:

- Fixed a black screen on iPhone and iPad when a game couldn't start. The app picker now shows why, and other games can still be started afterwards.
- On iPhone and iPad, when a game crashes, iDared 32bit goes back to the app picker and shows what went wrong, with an OK button.
- The app picker shows games' higher-resolution icons when they include them, so they look sharper on modern screens. Games with only a low-resolution icon have it scaled up more smoothly.
- Game icons are shown as they are, without an added glossy shine.
- On macOS, Windows, Android and Linux, the folders and files have the same simpler names as on iPhone and iPad: `Games`, `Saves`, `Options.txt`, `Wallpaper.png` and `Log.txt`. Existing ones are moved to the new names automatically. The Windows download includes an empty `Games` folder.
- The macOS app is called iDared 32bit, and keeps its data in an iDared 32bit folder in Application Support. The old touchHLE folder there is moved automatically.
- The app picker shows the status bar (the time and battery) again.

Compatibility:

- Fixed a crash in some games that show web pages.

Other:

- The folder of libraries that games use is now called `dylibs` instead of `touchHLE_dylibs`.

## v1.4.2 (2026-10-14)

Usability:

- The folders and files in the Files app have simpler names: `Games`, `Saves`, `Options.txt`, `Wallpaper.png` and `Log.txt`. Existing ones are moved to the new names automatically.
- Messages in the app and in `Log.txt` say iDared 32bit instead of touchHLE.
- `OPTIONS_HELP.txt` and the explanation at the top of `Options.txt` are rewritten to talk about iDared 32bit and games. Existing files are updated automatically, keeping your own options.

Other:

- The About screen asks you to contact iDared 32bit's support, not touchHLE, with questions, problems and compatibility results.
- On iPhone and iPad, the license information now includes the license notices of the libraries and fonts that games use inside the emulated device, and the written offer for the source code of the GPL-licensed libgcc and libstdc++ libraries. These were missing there.

## v1.4.1 (2026-10-12)

Compatibility:

- Fixed crashes and hangs when quitting some games.
- Fixed crashes, static and missing music in some games that stream music.
- Fixed a crash when resuming some games.

Usability:

- After a crash, you can go back to the app picker.

## v1.4.0 (2026-10-11)

Compatibility:

- Various audio fixes: games that stream compressed music through audio queues now play it, AAC, MP3 and ALAC files appear compressed to apps as on a device, more WAV formats are supported, and asking for an audio property that doesn't exist gives an error instead of a crash.
- Taps work in games that keep a hidden window on top of their screen, and image views and labels no longer block taps meant for what's behind them.
- Core Animation: `CAScrollLayer`, 3D layer transforms, `sublayers` and `contentsRect`, and no unneeded animations for layers moved every frame.
- Implemented `NSInputStream` and `NSOutputStream` for files, `NSSortDescriptor`, `class_getInstanceMethod()`, `glDiscardFramebufferEXT()`, `+[UIView setAnimationsEnabled:]` and 32-bit little-endian bitmap contexts.
- Fixed a crash when quitting some apps while they're loading.
- Fixed crashes in apps that read text files to the end.
- Various smaller fixes.

Usability:

- iPad support: iDared 32bit runs natively on iPad using the whole screen, and games made for both iPhone and iPad run their iPad version.
- The app picker fills the screen on modern iPhones and iPads, fits more apps on each page, and has sharp text and a wallpaper that's no longer stretched.
- The on-screen Home button is now an X, and stays visible when a game fills the screen.
- Fixed a crash on launch on older devices with less memory.
- The app picker shows apps' localized names, as on a device, fits slightly longer names, and ends names that don't fit with "…".
- On iOS, the app picker is now native, with a new look.

Other:

- Clearer errors for files that fail to open and for damaged or badly decrypted app binaries.

## v1.3.0 (2026-10-03)

Based on touchHLE v0.3.0. Its changes are in [CHANGELOG.md](CHANGELOG.md).

Compatibility:

- Without network access, sockets behave as on a device in airplane mode, so apps that try to connect carry on instead of hanging. The socket API also handles more options and flags, non-blocking sockets and connection timeouts.
- Threads: `NSThread` reports its real state and supports `cancel` and `exit`, and `pthread_exit()` and the pthread scheduling functions are implemented. A thread waiting with a timeout is no longer mistaken for a deadlock. (`NSThread` `isExecuting` by alborrajo.)
- Audio: voice processing audio units, the audio route property, and the length and seeking functions of `ExtAudioFile`.
- Games built with the Airplay SDK no longer fail their integrity check at startup.
- `printf()` and `NSLog()` support left-justified fields.
- Implemented `dladdr()`, `inet_aton()` (by kylon), `CC_SHA256` and its incremental functions, `-[UIWindow windowLevel]`, `-[NSFileHandle synchronizeFile]` and `-[NSProcessInfo processorCount]`, and more OpenGL ES and Core Graphics functions and parameters.
- More accurate `-[UIFont leading]` (by abnormalmaps).
- Apps that support portrait now start in portrait.
- App icons listed with `CFBundleIcons`, as iOS 5 apps do, are shown in the app picker.
- Various smaller fixes.

Usability:

- Return to the app picker with Esc on macOS and Windows, Back on Android, or by holding a controller's Back button. On iOS, there's an on-screen Home button beside the app. Apps are told they're quitting first.
- The app picker pages by swiping instead of with arrow buttons, and highlights the icon you press.

Other:

- The iOS app requires iOS 16 or later.
- The app's version is set only in `iphone/Config.xcconfig`, and `dev-scripts/prepare-release.sh` prepares a release.

## v1.2.23 (2026-09-29)

Compatibility:

- Fixed a black screen on iOS in games that bind their renderbuffer once and then present every frame.

Usability:

- The log from the previous run is now kept as `touchHLE_log.previous.txt`, so relaunching after a hang or black screen no longer loses the log of what went wrong.

Documentation:

- `CONTRIBUTING.md` is rewritten for iDared 32bit: contributions are ordinary pull requests on GitHub, to the iDared 32bit repository. The rules on copyright and reverse engineering are kept.
- `CODE_OF_CONDUCT.md` is iDared 32bit's own, replacing touchHLE's.
- touchHLE's GitHub issue templates are removed.
- `dev-docs/building.md` now covers building the iOS app with Xcode, including signing through `Local.xcconfig`, no longer lists Boost as a prerequisite, and has fixed links.
- The README links iDared 32bit's website and shows its icon. It links touchHLE for reference only, and states that touchHLE is not the place for iDared 32bit support or app compatibility reports.
- The README and the license notices state that iDared 32bit is not affiliated with or endorsed by the touchHLE or mGBA projects.
- iDared 32bit's own changes are listed in this file. `CHANGELOG.md` is touchHLE's changelog, unchanged.

Other:

- The startup banner shows iDared 32bit and its website.
- The Android package is now `org.alexmartin.idared32bit`, matching iOS.
- The iOS Xcode project no longer needs Boost, which only the old Dynarmic CPU backend used.
- Windows builds link again: the mGBA CPU core used GCC built-in functions that MSVC doesn't have.
- CI runs on pushes and pull requests to the `idared32bit` branch.

## v1.1.23 (2026-09-24)

The first iDared 32bit release, based on touchHLE's development version after v0.2.3 (its NEXT section in [CHANGELOG.md](CHANGELOG.md)).

Compatibility:

- Photo Album support: `UIImageWriteToSavedPhotosAlbum` saves to `DCIM/100APPLE` like real iOS.

Documentation:

- The README is rebranded for iDared 32bit, the iOS app, and the in-app copyright information starts with a notice that iDared 32bit is a fork of touchHLE.

Other:

- Replaced the Dynarmic-based JIT CPU emulation backend with an ARM interpreter derived from [mGBA](https://github.com/mgba-emu/mgba)'s, vendored in `vendor/mgba_arm/` and extended from ARMv4T to ARMv7-A, Thumb-2 included, with VFPv3 (flush-to-zero, rounding modes and short vectors included) and Advanced SIMD (NEON), plus the armv7s additions (VFPv4 fused multiply-add, half-precision conversions and integer divide). Fat binaries' armv7s slices are now preferred, then armv7, then armv6. mGBA is licensed under the MPL-2.0, like touchHLE.
- Replaced OpenAL Soft with a built-in software audio mixer that outputs through SDL's audio subsystem. touchHLE's guest-facing `OpenAL.framework` and its Audio Toolbox playback (Audio Queue, Audio Services, Audio Unit) are now backed by this mixer instead of the OpenAL Soft C library, so OpenAL Soft is no longer a dependency or a submodule (`vendor/openal-soft/`).
- Distributed binaries are licensed under the MPL-2.0.
