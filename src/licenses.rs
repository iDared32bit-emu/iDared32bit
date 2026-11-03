/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Prints copyright, authorship and license information.

const IDARED_FORK_NOTICE: &str = "
iDared 32bit is a fork of touchHLE, with a few differences in this build:

  - CPU emulation uses an ARM interpreter derived from mGBA's (replaces
    Dynarmic JIT)
  - The guest-facing OpenAL uses SDL audio (replaces OpenAL Soft library)

The maintainer (@apexad) contributed to touchHLE until this App Store
release, which the touchHLE project did not want, and is no longer part of
that project. This build includes changes that are not in touchHLE.
iDared 32bit is not affiliated with or endorsed by the touchHLE project.
Please don't ask touchHLE for support with iDared 32bit, or submit results
to touchHLE's compatibility database. Contact support@idared32bit-emu.com
instead.

Source code for iDared 32bit, including the version of touchHLE it is based
on, is available at <https://github.com/iDared32bit-emu/iDared32bit>.

The copyright, authorship and license information below applies to iDared 32bit
in full.
";

const MAIN_COPYRIGHT: &str = "
iDared 32bit © 2026 iDared 32bit project contributors.

touchHLE © 2023–2026 touchHLE project contributors.
";

const MAIN_LICENSE: &str = "
This program, iDared 32bit, is licensed under the Mozilla Public License,
version 2.0 (MPL-2.0). You can obtain a copy of the license at
<https://mozilla.org/MPL/2.0/>, and the source code at the address given
above.

The iDared 32bit name, logo and app icon are © 2026 Alex Martin, all rights
reserved. They are not licensed under the MPL-2.0 or any other open-source
license.

This program is not licensed under the GNU General Public License (GPL). Its
own code, and every library it is built from, is licensed under MPL-2.0 or
under more permissive terms, as listed below.

Under the MPL-2.0, this program is provided on an \"as is\" basis, without
warranty of any kind; see sections 6 and 7 of the license.

Separately from the program itself, it carries copies of the libgcc and
libstdc++ libraries from Apple's iPhone OS SDK, which apps running inside the
emulated device link against. On iOS, these copies are embedded in the app's
executable file, as data. They are separate programs, licensed under the GNU
GPL version 2 or later. They run only inside the emulated device, are not
linked with iDared 32bit, and do not change its license. Their license notices,
and a written offer for their source code, are included with this program.
";

// See android/app/src/main/java/org/alexmartin/idared32bit/DocumentsProvider.kt
#[cfg(target_os = "android")]
const SKYLINE: &str = "
touchHLE for Android incorporates code originally from the Skyline emulator
project, licensed under MPL-2.0:

Copyright © 2022 Skyline Team and Contributors (https://github.com/skyline-emu/)
";

const RUST_DESCRIPTION: &str = "
touchHLE, and therefore this executable, incorporates the following Rust
libraries, which are copyright their respective authors and other contributors,
and licensed as follows:
";

/// Names, versions, authors and licenses of Rust crates depended on by release
/// binaries. See `build.rs` for how this is generated.
const RUST_DEPENDENCIES: &str = include_str!(concat!(env!("OUT_DIR"), "/rust_dependencies.txt"));

/// Rust libraries offered only under Apache-2.0 (not MIT).
const APACHE_ONLY_CRATES: &[&str] = &[
    "ab_glyph_rasterizer",
    "hound",
    "mach_object",
    "owned_ttf_parser",
];
/// Rust libraries with an extra license, whose text is shown below.
#[cfg(test)]
const EXTRA_LICENSE_CRATES: &[(&str, &str)] = &[
    ("encoding_rs", "BSD-3-Clause"),
    ("unicode-ident", "Unicode-3.0"),
];
const APACHE_2_0: &str = include_str!("licenses/Apache-2.0.txt");
/// encoding_rs's copyright line, then the license.
const BSD_3_CLAUSE: &str = include_str!("licenses/BSD-3-Clause-encoding_rs.txt");
const UNICODE_3_0: &str = include_str!("licenses/Unicode-3.0-unicode-ident.txt");
/// Rust libraries under BSD licenses, whose own texts are shown below.
#[cfg(test)]
const BSD_LICENSE_CRATES: &[(&str, &str)] = &[
    ("tiny-skia", "BSD-3-Clause"),
    ("tiny-skia-path", "BSD-3-Clause"),
    ("arrayref", "BSD-2-Clause"),
    ("subtle", "BSD-3-Clause"),
];
const BSD_3_CLAUSE_TINY_SKIA: &str = include_str!("licenses/BSD-3-Clause-tiny-skia.txt");
const BSD_2_CLAUSE_ARRAYREF: &str = include_str!("licenses/BSD-2-Clause-arrayref.txt");
const BSD_3_CLAUSE_SUBTLE: &str = include_str!("licenses/BSD-3-Clause-subtle.txt");
const SDL2_SUN_LIBM: &str = include_str!("licenses/Sun-libm.txt");
const SDL2_X11_IMKSTOUCS: &str = include_str!("licenses/MIT-imKStoUCS.txt");

const YUV2RGB_CONDITION_3: &str = "
For yuv2rgb, condition 3 reads: \"Neither the name of yuv2rgb nor the names of
its contributors may be used to endorse or promote products derived from this
software without specific prior written permission.\"";

/// `{}` is where the libxml2 license (MIT) is.
const RUST_MIT_DESCRIPTION: &str =
    "The Rust libraries listed as licensed under MIT, alone or as one of their
options, are used under the MIT license: the same terms as the libxml2 license
{}, with each library's listed authors as
its copyright holders.
";

const MGBA_DESCRIPTION: &str = "
iDared 32bit, and therefore this executable, incorporates a modified copy of
the ARM interpreter from the mGBA Game Boy Advance emulator (in
vendor/mgba_arm/), which this build extends from ARMv4T to ARMv7s.
mGBA is copyright © 2013–2024 Jeffrey Pfau, and is available under the Mozilla
Public License, version 2.0 -- the same license as the rest of this program.
The modified source files are published with the rest of iDared 32bit's source
code (see above). iDared 32bit is not affiliated with or endorsed by the mGBA
project.
";

const SDL2_DESCRIPTION: &str = "
touchHLE, and therefore this executable, incorporates the library SDL 2, which
is available under the following license:
";

const SDL2_LICENSE: &str = "
This software is provided 'as-is', without any express or implied
warranty.  In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.
3. This notice may not be removed or altered from any source distribution.
";

const STB_IMAGE: &str = "
touchHLE, and therefore this executable, incorporates the library stb_image,
which is available either as Public Domain or under the terms of the MIT
license.
";

const PVRTD_DESCRIPTION: &str = "
touchHLE, and therefore this executable, incorporates PVRTC decompression code
from the PowerVR SDK, which is available under the following license:
";

const PVRTD_LICENSE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/vendor/PVRTDecompress/LICENSE.md"
));

// When resource files are bundled with touchHLE in such a way that the user can
// read their license files directly, use this caveat.

const EXTERNAL_FILES_CAVEAT: &str = "
The following authorship, copyright and license information relates to this
executable. Please note that different licensing terms apply to the bundled
dynamic libraries (in dylibs/) and fonts (in touchHLE_fonts/). Please
consult the respective directories for more information.
";

// When resource files are bundled with touchHLE in such a way that only
// touchHLE can read the license file, use these notices.

// Apple's own offer is reproduced below for provenance, but GPLv2 3(c) only
// allows passing it along for noncommercial distribution, so this build makes
// its own 3(b) offer.
const INTERNAL_FSF_DYLIBS_DESCRIPTION: &str = "
This distribution of iDared 32bit includes binaries for the libgcc and libstdc++
libraries from the Free Software Foundation, as originally distributed by Apple:
";

const INTERNAL_FSF_DYLIBS_GPL_HEADING: &str = "
The following license applies only to the libgcc and libstdc++ libraries
described above. It does not apply to iDared 32bit, which is licensed under
the MPL-2.0.
";

const INTERNAL_FSF_DYLIBS_OFFER: &str = "
Written offer, valid for three years from the date you received this
distribution: on request, the maintainer of iDared 32bit will give any third
party a complete machine-readable copy of the corresponding source code for
the libgcc and libstdc++ binaries included here, for no more than the cost of
performing the distribution. That source is also published at:

  libgcc    <https://github.com/iDared32bit-emu/gcc> (tag gcc-5664)
  libstdc++ <https://github.com/iDared32bit-emu/libstdcxx> (tag libstdcxx-39)

Requests: admin@idared32bit-emu.com
";

const INTERNAL_ZLIB_DYLIB_DESCRIPTION: &str = "
This distribution of iDared 32bit includes binaries for zlib (libz), available under
the following license:
";

// We have a COPYING file for SQLite, but as the main source and resulting
// binary are simply Public Domain, we can save some space here.
const INTERNAL_SQLITE3_DYLIB_DESCRIPTION: &str = "
This distribution of iDared 32bit includes binaries for SQLite (libsqlite3),
available under Public Domain.
";

const INTERNAL_XML2_DYLIB_DESCRIPTION: &str = "
This distribution of iDared 32bit includes binaries for libxml2, available under
the following license:
";

const INTERNAL_LIBERATION_FONTS_DESCRIPTION: &str = "
This distribution of iDared 32bit includes Liberation fonts (Sans, Serif and
Mono), available under the following license:
";

const INTERNAL_NOTO_FONTS_DESCRIPTION: &str = "
This distribution of iDared 32bit includes Noto Sans CJK fonts, available under the
following license:
";

const INTERNAL_CAVEATBRUSH_FONTS_DESCRIPTION: &str = "
This distribution of iDared 32bit includes Caveat Brush fonts, available under the
following license:
";

/// The license files read below, embedded for iOS: users can't open the
/// app bundle, and the files aren't at these paths inside it.
fn embedded_in_file(path: &str) -> Option<&'static str> {
    macro_rules! file {
        ($path:literal) => {
            (
                $path,
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
            )
        };
    }
    const FILES: &[(&str, &str)] = &[
        file!("dylibs/README.md"),
        file!("dylibs/COPYING.libz"),
        file!("dylibs/COPYING.libxml2"),
        file!("touchHLE_fonts/LICENSE.liberation"),
        file!("touchHLE_fonts/LICENSE.noto"),
        file!("touchHLE_fonts/LICENSE.caveatbrush"),
    ];
    // The keys are the files' paths in the repository, which differ from
    // the iOS app bundle's for the fonts.
    let path = path.replacen(crate::paths::FONTS_DIR, "touchHLE_fonts", 1);
    FILES
        .iter()
        .find(|&&(p, _)| p == path)
        .map(|&(_, text)| text)
}

fn read_bundled_file(path: &str) -> String {
    debug_assert!(
        embedded_in_file(path).is_some(),
        "{path} must be in embedded_in_file() for iOS",
    );
    if cfg!(target_os = "ios") {
        return embedded_in_file(path).unwrap().to_owned();
    }
    use std::io::Read;
    let mut res = String::new();
    crate::paths::ResourceFile::open(path)
        .unwrap()
        .get()
        .read_to_string(&mut res)
        .unwrap();
    res
}

fn divider(out: &mut String) -> Result<(), std::fmt::Error> {
    use std::fmt::Write;
    writeln!(out, "---")
}

fn print(out: &mut String, resources_are_external_files: bool) -> Result<(), std::fmt::Error> {
    use std::fmt::Write;
    writeln!(out, "{IDARED_FORK_NOTICE}")?;
    divider(out)?;
    if resources_are_external_files {
        writeln!(out, "{EXTERNAL_FILES_CAVEAT}")?;
        divider(out)?;
    }
    writeln!(out, "{MAIN_COPYRIGHT}")?;
    divider(out)?;
    writeln!(out, "{MAIN_LICENSE}")?;
    divider(out)?;
    #[cfg(target_os = "android")]
    {
        writeln!(out, "{SKYLINE}")?;
        divider(out)?;
    }
    writeln!(out, "{RUST_DESCRIPTION}")?;
    writeln!(out, "{RUST_DEPENDENCIES}")?;
    let libxml2_license = if resources_are_external_files {
        format!("in {}/COPYING.libxml2", crate::paths::DYLIBS_DIR)
    } else {
        "shown in this document".to_string()
    };
    writeln!(
        out,
        "{}",
        RUST_MIT_DESCRIPTION.replace("{}", &libxml2_license)
    )?;
    divider(out)?;
    writeln!(
        out,
        "\nThe following are licensed under the Apache License, version 2.0:\n",
    )?;
    for name in APACHE_ONLY_CRATES {
        writeln!(out, "- {name} (Rust library)")?;
    }
    writeln!(out, "- the Kotlin standard library (Android only)")?;
    writeln!(
        out,
        "- LLVM's libc++, libc++_shared.so (Android only), with the LLVM exception",
    )?;
    writeln!(out, "\n{}", APACHE_2_0.trim_end())?;
    divider(out)?;
    // Line by line, as Git may give the file Windows line endings.
    let (encoding_rs_copyright, bsd_3_clause) = BSD_3_CLAUSE.split_once('\n').unwrap();
    let (encoding_rs_copyright, bsd_3_clause) =
        (encoding_rs_copyright.trim_end(), bsd_3_clause.trim_start());
    writeln!(
        out,
        "\nThe following are licensed under the BSD-3-Clause license:\n\n\
         - data in encoding_rs (Rust library):\n  {encoding_rs_copyright}\n\
         - yuv2rgb, part of SDL 2:\n  Copyright (c) 2016, Adrien Descamps\n  \
         All rights reserved.\n",
    )?;
    writeln!(out, "{}", bsd_3_clause.trim_end())?;
    writeln!(out, "{YUV2RGB_CONDITION_3}")?;
    divider(out)?;
    writeln!(
        out,
        "\nThe Rust library unicode-ident also includes data under the following\n\
         license (Unicode-3.0):\n",
    )?;
    writeln!(out, "{}", UNICODE_3_0.trim_end())?;
    divider(out)?;
    writeln!(
        out,
        "\nThe Rust libraries tiny-skia and tiny-skia-path are licensed under the\n\
         following license (BSD-3-Clause):\n",
    )?;
    writeln!(out, "{}", BSD_3_CLAUSE_TINY_SKIA.trim_end())?;
    divider(out)?;
    writeln!(
        out,
        "\nThe Rust library arrayref is licensed under the following license\n\
         (BSD-2-Clause):\n",
    )?;
    writeln!(out, "{}", BSD_2_CLAUSE_ARRAYREF.trim_end())?;
    divider(out)?;
    writeln!(
        out,
        "\nThe Rust library subtle is licensed under the following license\n\
         (BSD-3-Clause):\n",
    )?;
    writeln!(out, "{}", BSD_3_CLAUSE_SUBTLE.trim_end())?;
    divider(out)?;
    writeln!(out, "{MGBA_DESCRIPTION}")?;
    divider(out)?;
    writeln!(out, "{SDL2_DESCRIPTION}")?;
    writeln!(out, "{SDL2_LICENSE}")?;
    writeln!(
        out,
        "SDL 2 also includes math functions under the following notice:\n\n{}\n",
        SDL2_SUN_LIBM.trim_end(),
    )?;
    writeln!(
        out,
        "In Linux builds only, SDL 2 also includes X11 keyboard code under the\n\
         following license:\n\n{}",
        SDL2_X11_IMKSTOUCS.trim_end(),
    )?;
    divider(out)?;
    writeln!(out, "{STB_IMAGE}")?;
    divider(out)?;
    writeln!(out, "{PVRTD_DESCRIPTION}")?;
    writeln!(out, "{}", PVRTD_LICENSE.trim_end())?;
    if !resources_are_external_files {
        divider(out)?;
        writeln!(out, "{INTERNAL_FSF_DYLIBS_DESCRIPTION}")?;
        writeln!(
            out,
            "{}.",
            read_bundled_file(&format!("{}/README.md", crate::paths::DYLIBS_DIR))
                // Skip preamble
                .split_once("## Original Apple license acknowledgements")
                .unwrap()
                .1
                // Don't include the whole GPL
                .split_once("; a copy of the GPL is included below.")
                .unwrap()
                .0
                // Remove one level of Markdown quoting
                .replace("\n> ", "\n")
                .trim_start()
        )?;
        writeln!(out, "{INTERNAL_FSF_DYLIBS_OFFER}")?;
        writeln!(out, "{INTERNAL_FSF_DYLIBS_GPL_HEADING}")?;
        // The GPL's full text, as plain text (it's quoted twice, in bold).
        let readme = read_bundled_file(&format!("{}/README.md", crate::paths::DYLIBS_DIR));
        let gpl = readme
            .split_once("; a copy of the GPL is included below.")
            .unwrap()
            .1;
        for line in gpl.trim_start_matches(['\n', '>', ' ']).lines() {
            let line = line.trim_start_matches(['>', ' ']).replace("**", "");
            writeln!(out, "{line}")?;
        }
        divider(out)?;
        writeln!(out, "{INTERNAL_ZLIB_DYLIB_DESCRIPTION}")?;
        writeln!(
            out,
            "{}",
            read_bundled_file(&format!("{}/COPYING.libz", crate::paths::DYLIBS_DIR))
        )?;
        divider(out)?;
        writeln!(out, "{INTERNAL_SQLITE3_DYLIB_DESCRIPTION}")?;
        divider(out)?;
        writeln!(out, "{INTERNAL_XML2_DYLIB_DESCRIPTION}")?;
        writeln!(
            out,
            "{}",
            read_bundled_file(&format!("{}/COPYING.libxml2", crate::paths::DYLIBS_DIR))
        )?;
        divider(out)?;
        writeln!(out, "{INTERNAL_LIBERATION_FONTS_DESCRIPTION}")?;
        writeln!(
            out,
            "{}",
            read_bundled_file(&format!("{}/LICENSE.liberation", crate::paths::FONTS_DIR))
        )?;
        divider(out)?;
        writeln!(out, "{INTERNAL_NOTO_FONTS_DESCRIPTION}")?;
        writeln!(
            out,
            "{}",
            read_bundled_file(&format!("{}/LICENSE.noto", crate::paths::FONTS_DIR))
        )?;
        divider(out)?;
        writeln!(out, "{INTERNAL_CAVEATBRUSH_FONTS_DESCRIPTION}")?;
        writeln!(
            out,
            "{}",
            read_bundled_file(&format!("{}/LICENSE.caveatbrush", crate::paths::FONTS_DIR))
        )?;
    }
    Ok(())
}

pub fn get_text() -> String {
    let mut string = String::new();
    print(&mut string, crate::paths::RESOURCES_ARE_EXTERNAL_FILES).unwrap();
    string
}

#[cfg(test)]
#[test]
fn test_external_files_license_text() {
    // The code above makes assumptions about the content of external files that
    // aren't checked at build time, and the runtime check only happens when you
    // view the license text on Android! This seems likely to silently break,
    // so it's a good candidate for a unit test.
    print(
        &mut String::new(),
        /* resources_are_external_files: */ true,
    )
    .unwrap();
}

#[cfg(test)]
#[test]
fn test_internal_files_license_text() {
    // What Android and iOS show, including the written offer for the GPL
    // libraries. read_bundled_file() checks iOS has each file embedded.
    let mut text = String::new();
    print(&mut text, /* resources_are_external_files: */ false).unwrap();
    assert!(text.contains(INTERNAL_FSF_DYLIBS_OFFER));
    // The GPL's full text, not just the notice.
    assert!(text.contains(INTERNAL_FSF_DYLIBS_GPL_HEADING));
    assert!(text.contains("END OF TERMS AND CONDITIONS"));
    assert!(!text.contains(EXTERNAL_FILES_CAVEAT));
    assert!(text.contains("the libxml2 license\nshown in this document"));
    for holder in [
        "WHATWG",
        "Adrien Descamps",
        "SunPro",
        "Jamey Sharp",
        "Kotlin",
        "Yevhenii Reizner",
        "David Roundy",
    ] {
        assert!(text.contains(holder), "{holder}");
    }
}

#[cfg(test)]
#[test]
fn test_rust_dependency_licenses_covered() {
    // Every Rust library's license must be one this text shows: MIT (as
    // the only license or an option), MPL-2.0, Zlib, or one listed above.
    for line in RUST_DEPENDENCIES.lines().filter(|l| l.starts_with("- ")) {
        let name = line[2..].split(' ').next().unwrap();
        let license = line.rsplit_once("licensed under ").unwrap().1;
        let mut license = license.replace("AND Zlib", "");
        for (extra_name, extra) in EXTRA_LICENSE_CRATES {
            let and_extra = format!("AND {extra}");
            if license.contains(&and_extra) {
                assert_eq!(name, *extra_name, "{line}");
                license = license.replace(&and_extra, "");
            }
        }
        assert!(
            !license.contains("AND"),
            "license text not shown for: {line}"
        );
        let license = license.trim();
        let covered = license.contains("MIT")
            || matches!(license, "MPL-2.0" | "Zlib")
            || (license == "Apache-2.0" && APACHE_ONLY_CRATES.contains(&name))
            || BSD_LICENSE_CRATES.contains(&(name, license));
        assert!(covered, "license text not shown for: {line}");
    }
}
