<p align="center"><img src="iphone/iDared32bit/Assets.xcassets/AppPickerLogo.imageset/AppPickerLogo@3x.png" alt="iDared 32bit logo" width="345"></p>

**iDared 32bit** runs classic 32-bit iPhone OS apps and games on modern iPhones and iPads.

It is a fork of **[touchHLE](https://github.com/touchHLE/touchHLE)**, a high-level emulator for early iPhone OS apps, packaged as a native iOS app.
The emulation is touchHLE's work; this fork adds the iOS host and the build-specific changes noted below.

Website (including the privacy policy): <https://idared32bit-emu.com/>

## What's different in this build

- **iOS host app** which runs as a native app on iPhone and iPad.
- **An interpreter instead of Dynarmic** for CPU emulation. The CPU core is an ARM interpreter derived from [mGBA](https://github.com/mgba-emu/mgba)'s, extended from ARMv4T to ARMv7s, with Thumb-2, VFPv3/VFPv4 and NEON. The only code run by the [emulated CPU](vendor/mgba_arm/) is the app binary and [a handful of libraries](dylibs/).
- **OpenAL via SDL Audio** for the guest-facing `OpenAL.framework` and Audio Toolbox playback, instead of the OpenAL Soft C library.
- **Changes not in upstream touchHLE** — the maintainer ([@apexad](https://github.com/apexad)) contributed to touchHLE until this App Store release, which the touchHLE project did not want, and is no longer part of that project. This build includes changes, some originally written for touchHLE, that enable games not supported upstream.

## Based on touchHLE

For reference only:

- Upstream project: <https://github.com/touchHLE/touchHLE>
- touchHLE's website: <https://touchhle.org/>
- touchHLE app compatibility database: <https://appdb.touchhle.org/>

These belong to touchHLE, not iDared 32bit. **Do not ask touchHLE for support with iDared 32bit, and do not submit iDared 32bit app compatibility results to touchHLE's website or its app compatibility database.** Please bring questions, problems and compatibility reports for iDared 32bit here instead.

The goal of this project is to run games from the early days of iOS:

* Currently: iPhone, iPod touch, and iPad apps for iPhone OS 2.x, iPhone OS 3.x and iOS 4.x.
* Later: iOS 5.x and beyond.
* [Never](https://github.com/touchHLE/touchHLE/issues/181#issuecomment-1777098259): 64-bit iOS.

**This does not mean that all apps for these OS versions work.** The vast majority of iPhone OS 2.x, iPhone OS 3.x and iOS 4.x apps do not currently work in touchHLE or iDared 32bit, and the ones that do work are generally games (support for other apps isn't a priority: it's more complex and less fun). This improves gradually over time with contributions from various developers. touchHLE's app compatibility database tracks touchHLE only; it does not cover iDared 32bit. **We don't take requests, so please do not ask us to support your favourite game.**

Because iDared 32bit uses a different CPU backend, SDL-based audio, iOS-specific changes, and additional maintainer changes that are not in upstream touchHLE, compatibility may differ from touchHLE. Some games that work in touchHLE may not work in iDared 32bit, and vice versa.

## Platform support

iDared 32bit is built, tested and released as a native **iOS** app for iPhone and iPad (AArch64).

* Officially supported:
  * iOS, released on the App Store.
  * AArch64 macOS, which is used for development. You must build it yourself.
* Inherited from touchHLE, but not verified in this fork: x64 macOS, x64 and AArch64 Linux, x64 Windows and AArch64 Android. You must build these yourself.

There are no binary releases except the App Store app. This fork's GitHub Actions only check formatting and run lint and tests; they don't build releases.

## Important disclaimers

This project is not affiliated with or endorsed by Apple Inc. in any way. Apple, iPhone, iPad, and iPod touch are trademarks of Apple Inc., registered in the U.S. and other countries and regions. iOS is a trademark or registered trademark of Cisco in the U.S. and other countries and is used under license by Apple.

Only use iDared 32bit to emulate software you have obtained legally.

Not an official touchHLE release, and not affiliated with or endorsed by the touchHLE project — please direct questions or problems regarding iDared 32bit here, not to the touchHLE project.

Not affiliated with or endorsed by the mGBA project, whose ARM interpreter iDared 32bit's CPU core is derived from — please direct questions or problems regarding iDared 32bit here, not to the mGBA project.

## Graphical user interface

On iPhone and iPad, iDared 32bit has its own native app picker; other platforms use the built-in app picker inherited from touchHLE. If you put your `.ipa` files and `.app` bundles in the `Games` folder, they will show up in the app picker when you run iDared 32bit. Note that the app binary must not be encrypted to be usable.
To access the `Games` folder, simply tap the “File manager” button in iDared 32bit.

On iPhone and iPad, you can also tap “Import” in the app picker to copy `.ipa` files or `.app` folders into `Games` from the Files app, or share an `.ipa` file to iDared 32bit from another app (“Open in iDared 32bit”).

To configure the options, you can edit the `Options.txt` file. To get a list of options, look in the `OPTIONS_HELP.txt` file.

On iPhone and iPad, Settings in the app picker has the most common options (such as the scale hack, orientation and network access) for the game you launch next. They start at “Default” each time, which leaves `Options.txt` in charge; anything else you pick overrides `Options.txt` for that launch only. To make a setting permanent for a game, put it in `Options.txt`.

Any data saved by the app (e.g. **saved games**) are stored in the `Saves` folder.

Older versions used touchHLE's names (`touchHLE_apps`, `touchHLE_options.txt`, `touchHLE_sandbox` and `touchHLE_wallpaper.png`), and iDared 32bit moves them to the new names automatically.

## License

iDared 32bit © 2026 iDared 32bit project contributors.

touchHLE © 2023–2026 touchHLE project contributors.

iDared 32bit is based on touchHLE. Its source code, including touchHLE's and the modifications made to it here, is licensed under the Mozilla Public License 2.0 (MPL-2.0). The iDared 32bit name, logo and app icon are not; see below.

**Distributed binaries are also licensed under the MPL-2.0. iDared 32bit is not licensed under the GNU General Public License (GPL).** Its own code, and every library it is built from, is licensed under the MPL-2.0 or under more permissive terms. Separately, the app carries two GPL-licensed libraries, described below.

The app carries copies of the libgcc and libstdc++ dynamic libraries from Apple's iPhone OS SDK (`dylibs/`), which apps running inside the emulated device link against. On iOS, these copies are embedded in the app's executable file, as data. They are separate programs, licensed under the GNU GPL version 2 or later. They run only inside the emulated device, are not linked with iDared 32bit, and do not change its license. Their license notices, the full text of the GPL, and a written offer for their source code, are included in the app's license information.

Copyright, authorship, and license notices for touchHLE, mGBA, and bundled components are preserved in the source and available through the app's license information. See `LICENSE`, `src/licenses.rs`, and the applicable bundled-component directories for details.

### Name, logo and app icon

The iDared 32bit name, logo and app icon are © 2026 Alex Martin, all rights reserved. They are not licensed under the MPL-2.0 or any other open-source license, and the MPL-2.0 grants no rights to them either (see its section 2.3). They are in this repository only so that iDared 32bit itself can be built. These files are covered:

- `iphone/iDared32bit/Assets.xcassets/AppIcon.appiconset/AppIcon.png` (the app icon)
- `res/icon.png` (a copy of the app icon, used by the other platforms)
- `iphone/iDared32bit/Assets.xcassets/AppPickerLogo.imageset/AppPickerLogo@3x.png` (the logo)

If you distribute your own build or fork, give it its own name, logo and icon, and don't present it as iDared 32bit. The code itself remains yours to use under the MPL-2.0.

# Thanks

We stand on the shoulders of giants. Thank you to:

* Everyone who has contributed to the project or supported any of its contributors financially.
* The authors of and contributors to the many libraries used by this project: [mGBA](https://github.com/mgba-emu/mgba) (whose ARM interpreter core, modified, is vendored in [vendor/mgba_arm/](vendor/mgba_arm/)), [rust-macho](https://github.com/flier/rust-macho), [SDL](https://libsdl.org/), [rust-sdl2](https://github.com/Rust-SDL2/rust-sdl2), [stb\_image](https://github.com/nothings/stb), Imagination Technologies' [PVRTC decompressor](https://github.com/powervr-graphics/Native_SDK/blob/master/framework/PVRCore/texture/PVRTDecompress.cpp), [hound](https://github.com/ruuda/hound), [Symphonia](https://github.com/pdeljanov/Symphonia), [RustType](https://gitlab.redox-os.org/redox-os/rusttype), [the Liberation fonts](https://github.com/liberationfonts/liberation-fonts), [the Noto CJK fonts](https://github.com/googlefonts/noto-cjk), [rust-plist](https://github.com/ebarnard/rust-plist), [nibarchive](https://github.com/michaelwright235/nibarchive), [quick-xml](https://github.com/tafia/quick-xml), [gl-rs](https://github.com/brendanzab/gl-rs), [cargo-license](https://github.com/onur/cargo-license), [cc-rs](https://github.com/rust-lang/cc-rs), [cmake-rs](https://github.com/rust-lang/cmake-rs), [cargo-ndk](https://github.com/bbqsrc/cargo-ndk), [cargo-ndk-android-gradle](https://github.com/willir/cargo-ndk-android-gradle), [md-5 and sha1](https://github.com/RustCrypto/hashes), [encoding_rs](https://github.com/hsivonen/encoding_rs), [tiny-skia](https://github.com/linebender/tiny-skia), [corosensei](https://github.com/Amanieu/corosensei), [uuid](https://github.com/uuid-rs/uuid) and the Rust standard library.
* The [Rust project](https://www.rust-lang.org/) generally.
* The various people out there who've documented the iPhone OS platform, officially or otherwise. Much of this documentation is linked to within this codebase!
* The iOS hacking/jailbreaking community.
* The Free Software Foundation, for making libgcc and libstdc++ copyleft and therefore saving this project from ABI hell.
* Many friends who took an interest in the project and gave suggestions and encouragement.
* Developers of early iPhone OS apps. What treasures you created!
* Apple, and NeXT before them, for creating such fantastic platforms.
