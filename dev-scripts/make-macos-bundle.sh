#!/bin/sh
set -e

# Creates the .app bundle containing the basic set of files needed for iDared
# 32bit to run. Also adds an icon and metadata similar to the Android APK.

if [[ $# == 3 ]]; then
    PATH_TO_BINARY="$1"
    VERSION="$2"
    BRANDING="$3"
    shift 3

    # iDared 32bit has one icon for every branding.
    ICON_NAME=icon
    if [[ "x$BRANDING" == "x" ]]; then
        APP_NAME="iDared 32bit"
    else
        APP_NAME="iDared 32bit $BRANDING"
        VERSION="$VERSION $BRANDING"
    fi
    rm -rf "$ICON_NAME.icns" "$ICON_NAME.iconset"
    mkdir "$ICON_NAME.iconset"
    cp ../res/"$ICON_NAME.png" "$ICON_NAME.iconset"/icon_512x512.png
    iconutil -c icns -o "$ICON_NAME.icns" "$ICON_NAME.iconset"

    rm -rf "$APP_NAME.app"
    mkdir -p "$APP_NAME.app"/Contents/MacOS "$APP_NAME.app"/Contents/Resources
    cp $PATH_TO_BINARY "$APP_NAME.app"/Contents/MacOS/iDared32bit
    cp -r ../dylibs "$APP_NAME.app"/Contents/Resources/
    cp -r ../fonts "$APP_NAME.app"/Contents/Resources/
    cp -r ../touchHLE_default_options.txt "$APP_NAME.app"/Contents/Resources/
    cp "$ICON_NAME.icns" "$APP_NAME.app"/Contents/Resources/

    plutil -create xml1 "$APP_NAME.app"/Contents/Info.plist
    plutil -insert CFBundleName -string "$APP_NAME" "$APP_NAME.app"/Contents/Info.plist
    plutil -insert CFBundleDisplayName -string "$APP_NAME" "$APP_NAME.app"/Contents/Info.plist
    plutil -insert CFBundleShortVersionString -string "$VERSION" "$APP_NAME.app"/Contents/Info.plist
    plutil -insert CFBundleExecutable -string iDared32bit "$APP_NAME.app"/Contents/Info.plist
    plutil -insert CFBundleIconFile -string "$ICON_NAME" "$APP_NAME.app"/Contents/Info.plist
else
    echo "Incorrect usage."
    exit 1
fi
