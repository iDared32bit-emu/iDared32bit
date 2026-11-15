#!/bin/sh
set -e

# Bundles the iDared 32bit executable with the basic set of files needed for
# it to run (the same ones found in the macOS .app bundle or Android APK).
# This does not prepare a full release.

if [[ $# == 1 ]]; then
    PATH_TO_BINARY="$1"
    shift

    rm -rf iDared32bit_windows_bundle
    mkdir iDared32bit_windows_bundle
    cp $PATH_TO_BINARY iDared32bit_windows_bundle/iDared32bit.exe
    cp -r ../dylibs iDared32bit_windows_bundle/
    cp -r ../fonts iDared32bit_windows_bundle/
    cp -r ../touchHLE_default_options.txt iDared32bit_windows_bundle/
    # Where games go. Windows has no fixed place to create it in.
    mkdir iDared32bit_windows_bundle/Games
else
    echo "Incorrect usage."
    exit 1
fi
