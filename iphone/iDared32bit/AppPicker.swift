/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// Native SwiftUI app picker, shown by src/environment/app_picker/swiftui.rs.
// The layout matches the picker drawn by src/environment/app_picker.rs.

import Combine
import CoreImage
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers
import UIKit

// MARK: - Entry point

struct PickerInput: Decodable {
    struct App: Decodable {
        let name: String
        let iconWidth: Int
        let iconHeight: Int
        let hasIcon: Bool
    }
    /// A choice (`choices` set) or a switch. Values are a choice's index, or
    /// 1 for a switch that's on.
    struct QuickOption: Decodable {
        let label: String
        let footer: String
        let choices: [String]?
        let defaultValue: Int
    }
    let apps: [App]
    let error: String?
    let wallpaperPath: String?
    /// Where the wallpaper goes, and the names Rust looks for there.
    let wallpaperFolder: String
    let wallpaperFiles: [String]
    /// The Games folder, watched for games added while the picker is open.
    let appsFolder: String
    /// Kept from before the picker was shown again.
    let initialPage: Int
    let initialQuickOptionValues: [Int]?
    let branding: String
    let shortVersionText: String
    let copyright: String
    let fileManagerURL: String?
    let websiteURL: String
    let quickOptions: [QuickOption]
    /// Why the last game couldn't start, if it couldn't.
    let startError: String?
}

/// Games opened with "Open in iDared 32bit", waiting for the picker.
enum OpenedGames {
    /// Only used on the main thread, where UIKit calls the hook below.
    nonisolated(unsafe) static var pending: [URL] = []
    static let notification = Notification.Name("IDaredOpenedGames")

    static func take() -> [URL] {
        defer { pending = [] }
        return pending
    }
}

/// Called from main() before SDL starts. SDL turns files opened in the app
/// into drop events, which nothing reads, so this also gives them to the
/// picker. The app opens them in place, so they aren't put in an Inbox.
@_cdecl("idared_install_open_in_hook")
public func idaredInstallOpenInHook() {
    let selector = NSSelectorFromString("sendDropFileForURL:")
    for name in ["SDLUIKitSceneDelegate", "SDLUIKitDelegate"] {
        guard let delegateClass = NSClassFromString(name),
              let method = class_getInstanceMethod(delegateClass, selector)
        else {
            continue
        }
        typealias Original = @convention(c) (AnyObject, Selector, NSURL) -> Void
        let original = unsafeBitCast(method_getImplementation(method), to: Original.self)
        let replacement: @convention(block) (AnyObject, NSURL) -> Void = { delegate, url in
            if url.isFileURL {
                OpenedGames.pending.append(url as URL)
                NotificationCenter.default.post(name: OpenedGames.notification, object: nil)
            }
            original(delegate, selector, url)
        }
        method_setImplementation(method, imp_implementationWithBlock(replacement))
    }
}

/// Called from Rust. Shows the picker until an app is picked, then returns
/// the app's index and the option arguments, one per line, from `strdup()`.
@_cdecl("idared_swiftui_app_picker")
public func idaredSwiftUIAppPicker(
    _ json: UnsafePointer<CChar>,
    _ icons: UnsafePointer<UnsafePointer<UInt8>?>,
    _ iconCount: Int
) -> UnsafeMutablePointer<CChar>? {
    let data = Data(bytes: json, count: strlen(json))
    guard let input = try? JSONDecoder().decode(PickerInput.self, from: data)
    else {
        return nil
    }
    var appIcons: [UIImage] = []
    for (i, app) in input.apps.enumerated() {
        guard i < iconCount, let pixels = icons[i],
              let icon = makeImage(pixels, app.iconWidth, app.iconHeight)
        else {
            return nil
        }
        appIcons.append(app.hasIcon ? upscaleSmallIcon(icon) : addQuestionMark(icon))
    }

    let model = PickerModel(input: input, icons: appIcons)
    let window: UIWindow
    if let scene = activeWindowScene() {
        window = UIWindow(windowScene: scene)
    } else {
        window = UIWindow(frame: UIScreen.main.bounds)
    }
    // Above SDL's launch screen, which fades out on its own.
    window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue + 2)
    window.rootViewController = PickerHostingController(model: model)
    window.makeKeyAndVisible()

    // Rust is waiting on the main thread, so keep the run loop going here.
    while model.result == nil {
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))
    }
    // Let the picked icon's highlight show before the app starts.
    RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))

    window.isHidden = true
    window.rootViewController = nil
    return strdup(model.result!)
}

private func activeWindowScene() -> UIWindowScene? {
    let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
    return scenes.first { $0.activationState == .foregroundActive }
        ?? scenes.first { $0.activationState == .foregroundInactive }
        ?? scenes.first
}

/// Rust's images are premultiplied RGBA, top row first.
private func makeImage(_ pixels: UnsafePointer<UInt8>, _ width: Int, _ height: Int) -> UIImage? {
    let data = Data(bytes: pixels, count: width * height * 4)
    guard let provider = CGDataProvider(data: data as CFData),
          let image = CGImage(
              width: width,
              height: height,
              bitsPerComponent: 8,
              bitsPerPixel: 32,
              bytesPerRow: width * 4,
              space: CGColorSpace(name: CGColorSpace.sRGB)!,
              bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
              provider: provider,
              decode: nil,
              shouldInterpolate: true,
              intent: .defaultIntent
          )
    else {
        return nil
    }
    return UIImage(cgImage: image)
}

private let iconContext = CIContext()

/// Games with only an old 57px icon look blocky when it's stretched to the
/// picker's size, so it's first doubled with smooth (bilinear) scaling.
/// Bigger icons are left as they are.
private func upscaleSmallIcon(_ icon: UIImage) -> UIImage {
    guard let cgImage = icon.cgImage, cgImage.width <= 57 else {
        return icon
    }
    let rect = CGRect(x: 0, y: 0, width: cgImage.width * 2, height: cgImage.height * 2)
    // Clamped, so the edges don't fade into the transparency around them.
    let output = CIImage(cgImage: cgImage)
        .clampedToExtent()
        .transformed(by: CGAffineTransform(scaleX: 2, y: 2))
        .cropped(to: rect)
    guard let scaled = iconContext.createCGImage(output, from: rect) else {
        return icon
    }
    return UIImage(cgImage: scaled)
}

private func addQuestionMark(_ icon: UIImage) -> UIImage {
    UIGraphicsImageRenderer(size: icon.size).image { _ in
        icon.draw(at: .zero)
        let text = "?" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 40),
            .foregroundColor: UIColor.white,
        ]
        let size = text.size(withAttributes: attributes)
        text.draw(
            at: CGPoint(x: (icon.size.width - size.width) / 2, y: (icon.size.height - size.height) / 2),
            withAttributes: attributes
        )
    }
}

// MARK: - Model

final class PickerModel: ObservableObject {
    let input: PickerInput
    let icons: [UIImage]
    @Published var wallpaper: UIImage?

    @Published var safeInsets = UIEdgeInsets.zero
    @Published var page = 0
    @Published var pressedApp: Int?
    @Published var showingSettings = false
    @Published var showingCopyright = false
    @Published var copyrightPage = 0
    @Published var showingStartError: Bool
    var copyrightPages: [String] = []

    /// The value of each of the quick options.
    @Published var quickOptionValues: [Int]

    @Published var headerColor = HeaderColor.saved {
        didSet {
            UserDefaults.standard.set(headerColor.rawValue, forKey: headerColorKey)
        }
    }

    @Published var hideHeaderLogo = UserDefaults.standard.bool(forKey: hideHeaderLogoKey) {
        didSet {
            UserDefaults.standard.set(hideHeaderLogo, forKey: hideHeaderLogoKey)
        }
    }

    @Published var hideIconLabels = UserDefaults.standard.bool(forKey: hideIconLabelsKey) {
        didSet {
            UserDefaults.standard.set(hideIconLabels, forKey: hideIconLabelsKey)
        }
    }

    /// Whether the footer behind the bottom bar has the header's background.
    @Published var footerBackground = UserDefaults.standard.bool(forKey: footerBackgroundKey) {
        didSet {
            UserDefaults.standard.set(footerBackground, forKey: footerBackgroundKey)
        }
    }

    /// The logo is dimmed and black and white on the black header.
    var blackHeaderShown: Bool {
        headerColor == .black
    }

    @Published var headerTextColor = HeaderTextColor.saved {
        didSet {
            UserDefaults.standard.set(headerTextColor.rawValue, forKey: headerTextColorKey)
        }
    }

    /// Set when an app is picked: what's returned to Rust.
    var result: String?

    init(input: PickerInput, icons: [UIImage]) {
        self.input = input
        self.icons = icons
        quickOptionValues = input.quickOptions.map(\.defaultValue)
        showingStartError = input.startError != nil
        wallpaper = input.wallpaperPath.flatMap { UIImage(contentsOfFile: $0) }
        if let values = input.initialQuickOptionValues,
           values.count == quickOptionValues.count {
            quickOptionValues = values
        }
        page = input.initialPage
        gamesFolderContents = Self.contents(of: input.appsFolder)
        looseGames = Self.looseGames(beside: input.appsFolder)
    }

    /// Saves a photo as the wallpaper, in place of any other wallpaper file.
    func setWallpaper(_ photo: UIImage) {
        let image = screenSized(photo)
        guard let data = image.jpegData(compressionQuality: 0.9) else {
            return
        }
        removeWallpaperFiles()
        // One of the names in wallpaperFiles.
        let url = URL(fileURLWithPath: input.wallpaperFolder)
            .appendingPathComponent("Wallpaper.jpg")
        do {
            try data.write(to: url)
            wallpaper = image
        } catch {
            NSLog("Couldn't save the wallpaper: %@", error.localizedDescription)
        }
    }

    func removeWallpaper() {
        removeWallpaperFiles()
        wallpaper = nil
    }

    private func removeWallpaperFiles() {
        let folder = URL(fileURLWithPath: input.wallpaperFolder)
        for name in input.wallpaperFiles {
            try? FileManager.default.removeItem(at: folder.appendingPathComponent(name))
        }
    }

    /// Photos can be far bigger than the screen, which the wallpaper only
    /// fills, so shrink them until their short side covers its long side.
    private func screenSized(_ photo: UIImage) -> UIImage {
        let screen = UIScreen.main.nativeBounds.size
        let pixels = CGSize(
            width: photo.size.width * photo.scale,
            height: photo.size.height * photo.scale
        )
        let factor = max(screen.width, screen.height) / min(pixels.width, pixels.height)
        guard factor < 1 else {
            return photo
        }
        let size = CGSize(
            width: (pixels.width * factor).rounded(),
            height: (pixels.height * factor).rounded()
        )
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            photo.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    @Published var showingImporter = false
    /// Games to import that are already in Games, while asking what to do.
    @Published var importConflicts: [String] = []
    @Published var importError: String?
    private var pendingImport: [(url: URL, move: Bool)] = []
    /// Security-scoped URLs from the Files picker, accessed until copied.
    private var accessedURLs: [URL] = []

    private var gamesFolder: URL {
        URL(fileURLWithPath: input.appsFolder)
    }

    /// Where iOS puts copies of games shared with the app, e.g. from Mail.
    private var inboxFolder: URL {
        gamesFolder.deletingLastPathComponent().appendingPathComponent("Inbox")
    }

    /// Imports .ipa files and .app folders picked in Files. A picked folder
    /// that isn't a game imports the games inside it.
    func importPicked(_ urls: [URL]) {
        for url in urls where url.startAccessingSecurityScopedResource() {
            accessedURLs.append(url)
        }
        importGames(urls.flatMap(Self.games(in:)).map { (url: $0, move: false) })
    }

    /// Imports games opened with "Open in iDared 32bit": copies of the
    /// originals, and anything iOS put in the Inbox, which is moved.
    func importSharedGames() {
        // Not while another import is copying games.
        guard pendingImport.isEmpty else {
            return
        }
        let opened = OpenedGames.take().filter { Self.isGame($0) }
        for url in opened where url.startAccessingSecurityScopedResource() {
            accessedURLs.append(url)
        }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: inboxFolder.path)) ?? []
        let inbox = names
            .map { inboxFolder.appendingPathComponent($0) }
            .filter { Self.isGame($0) }
        let games = opened.map { (url: $0, move: false) } + inbox.map { (url: $0, move: true) }
        guard !games.isEmpty else {
            reloadIfGamesChanged()
            return
        }
        importGames(games)
    }

    private static func isGame(_ url: URL) -> Bool {
        ["ipa", "app"].contains(url.pathExtension.lowercased())
    }

    private static func games(in url: URL) -> [URL] {
        if isGame(url) {
            return [url]
        }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: url.path)) ?? []
        return names.map { url.appendingPathComponent($0) }.filter(isGame)
    }

    private func importGames(_ games: [(url: URL, move: Bool)]) {
        // A game shared from Games itself is already there.
        let games = games.filter {
            $0.url.deletingLastPathComponent().standardizedFileURL.path
                != gamesFolder.standardizedFileURL.path
        }
        guard !games.isEmpty else {
            stopAccessing()
            importError = "There were no games (.ipa files or .app folders) to import."
            return
        }
        pendingImport = games
        let existing = games.map(\.url.lastPathComponent).filter { name in
            FileManager.default.fileExists(atPath: gamesFolder.appendingPathComponent(name).path)
        }
        if existing.isEmpty {
            finishImport(replacing: false)
        } else {
            importConflicts = existing
        }
    }

    /// Copies (or moves) the pending games into Games, off the main thread,
    /// then shows the picker again with them.
    func finishImport(replacing: Bool) {
        let games = pendingImport
        let folder = gamesFolder
        Task.detached {
            let manager = FileManager.default
            var failures: [String] = []
            try? manager.createDirectory(at: folder, withIntermediateDirectories: true)
            for (url, move) in games {
                let destination = folder.appendingPathComponent(url.lastPathComponent)
                do {
                    if manager.fileExists(atPath: destination.path) {
                        guard replacing else {
                            if move {
                                try? manager.removeItem(at: url)
                            }
                            continue
                        }
                        try manager.removeItem(at: destination)
                    }
                    if move {
                        try manager.moveItem(at: url, to: destination)
                    } else {
                        try manager.copyItem(at: url, to: destination)
                    }
                } catch {
                    failures.append("\(url.lastPathComponent): \(error.localizedDescription)")
                }
            }
            // A copy: the loop above is done changing it.
            await MainActor.run { [failures] in
                self.pendingImport = []
                self.stopAccessing()
                if failures.isEmpty {
                    self.reloadIfGamesChanged()
                } else {
                    self.importError = failures.joined(separator: "\n")
                }
            }
        }
    }

    func cancelImport() {
        // Shared games that weren't imported aren't wanted in the Inbox.
        for (url, move) in pendingImport where move {
            try? FileManager.default.removeItem(at: url)
        }
        pendingImport = []
        stopAccessing()
    }

    private func stopAccessing() {
        for url in accessedURLs {
            url.stopAccessingSecurityScopedResource()
        }
        accessedURLs = []
    }

    /// What was in the Games folder when the picker was shown.
    private var gamesFolderContents: [String] = []
    /// The .ipa files in the main folder then, which Rust moves into Games.
    private var looseGames: [String] = []

    private static func looseGames(beside gamesFolder: String) -> [String] {
        let mainFolder = (gamesFolder as NSString).deletingLastPathComponent
        let names = (try? FileManager.default.contentsOfDirectory(atPath: mainFolder)) ?? []
        return names.filter { ($0 as NSString).pathExtension.lowercased() == "ipa" }.sorted()
    }

    /// The names and modification dates of what's in a folder.
    private static func contents(of folder: String) -> [String] {
        let manager = FileManager.default
        let names = (try? manager.contentsOfDirectory(atPath: folder)) ?? []
        return names.sorted().map { name in
            let path = (folder as NSString).appendingPathComponent(name)
            let date = (try? manager.attributesOfItem(atPath: path))?[.modificationDate]
            return "\(name) \(String(describing: date))"
        }
    }

    /// Asks Rust to show the picker again if games were added or removed,
    /// e.g. in the Files app, keeping the page and options.
    func reloadIfGamesChanged() {
        guard result == nil,
              Self.contents(of: input.appsFolder) != gamesFolderContents
                  || Self.looseGames(beside: input.appsFolder) != looseGames
        else {
            return
        }
        let lines = ["reload", String(page)] + quickOptionValues.map(String.init)
        result = lines.joined(separator: "\n")
    }

    /// Rust turns the quick option values into option arguments.
    func pick(_ app: Int) {
        let lines = [String(app)] + quickOptionValues.map(String.init)
        result = lines.joined(separator: "\n")
    }

    func open(_ url: String?) {
        guard let url = url.flatMap({ URL(string: $0) }) else {
            return
        }
        UIApplication.shared.open(url)
    }
}

final class PickerHostingController: UIHostingController<PickerView> {
    private let model: PickerModel

    init(model: PickerModel) {
        self.model = model
        super.init(rootView: PickerView(model: model))
        view.backgroundColor = .black
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    // The time and battery, over the header. Games still hide it.
    override var prefersStatusBarHidden: Bool { false }

    override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }

    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        .portrait
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        model.safeInsets = view.safeAreaInsets
    }
}

// MARK: - Layout

/// The logo's background colour, so it blends in.
let headerBlue = Color(red: 0, green: 77 / 255, blue: 215 / 255)
let logoYellow = Color(red: 253 / 255, green: 224 / 255, blue: 22 / 255)

private let headerColorKey = "AppPickerHeaderColor"
private let hideHeaderLogoKey = "AppPickerHideHeaderLogo"
private let hideIconLabelsKey = "AppPickerHideIconLabels"
private let footerBackgroundKey = "AppPickerFooterBackground"
private let headerTextColorKey = "AppPickerHeaderTextColor"

/// The header text colors to choose from, for the version, name and About.
enum HeaderTextColor: String, CaseIterable {
    case yellow, white, gray, black, purple, orange, teal

    static var saved: HeaderTextColor {
        UserDefaults.standard.string(forKey: headerTextColorKey)
            .flatMap(HeaderTextColor.init(rawValue:)) ?? .yellow
    }

    var color: Color {
        switch self {
        case .white: return .white
        case .gray: return Color(white: 0.65)
        case .yellow: return logoYellow
        case .black: return .black
        case .purple: return HeaderColor.purple.color
        case .orange: return HeaderColor.orange.color
        case .teal: return HeaderColor.teal.color
        }
    }

    var name: String {
        rawValue.capitalized
    }
}

/// The header colors to choose from.
enum HeaderColor: String, CaseIterable {
    case blue, black, purple, pink, green, orange, teal

    static var saved: HeaderColor {
        UserDefaults.standard.string(forKey: headerColorKey)
            .flatMap(HeaderColor.init(rawValue:)) ?? .blue
    }

    var color: Color {
        switch self {
        case .blue: return headerBlue
        case .black: return .black
        case .purple: return Color(red: 0.42, green: 0.18, blue: 0.75)
        case .pink: return Color(red: 0.88, green: 0.24, blue: 0.55)
        case .green: return Color(red: 0.08, green: 0.55, blue: 0.28)
        case .orange: return Color(red: 0.8, green: 0.33, blue: 0)
        case .teal: return Color(red: 0, green: 0.55, blue: 0.55)
        }
    }

    var name: String {
        rawValue.capitalized
    }
}
let labelFontSize: CGFloat = 14
let labelLineHeight: CGFloat = 17

/// Everything is placed by its frame, as in the Rust picker.
extension View {
    func place(_ rect: CGRect) -> some View {
        frame(width: rect.width, height: rect.height)
            .position(x: rect.midX, y: rect.midY)
    }
}

struct PickerLayout {
    let size: CGSize
    let header: CGRect
    /// The row inside the header with the logo and buttons.
    let headerContent: CGRect
    let insets: UIEdgeInsets
    let bar: CGRect
    let gridFrame: CGRect
    let pageControlFrame: CGRect
    let columns: Int
    let rows: Int
    let iconSize: CGFloat
    let columnWidth: CGFloat
    let rowHeight: CGFloat
    let labelWidth: CGFloat
    /// About two characters wider, used when the neighbouring names leave room.
    let labelMaxWidth: CGFloat

    init(size: CGSize, insets: UIEdgeInsets, hideLogo: Bool) {
        self.size = size
        self.insets = insets
        // Phones with a notch or Dynamic Island leave about 10pt below it in
        // their safe area; use that. Older ones only leave room for the
        // status bar.
        let headerTop = insets.top > 40 ? insets.top - 10 : max(insets.top, 8)
        // Without the logo, it only needs to fit the buttons.
        let headerHeight: CGFloat = hideLogo ? 62 : 72
        headerContent = CGRect(x: 0, y: headerTop, width: size.width, height: headerHeight)
        header = CGRect(x: 0, y: 0, width: size.width, height: headerContent.maxY + 6)

        // Sized and placed like the iOS 26 tab bar.
        let barBottom = size.height - (insets.bottom > 0 ? max(insets.bottom - 15, 12) : 12)
        let barWidth = min(size.width - 32, 330)
        bar = CGRect(
            x: (size.width - barWidth) / 2,
            y: barBottom - 64,
            width: barWidth,
            height: 64
        )
        pageControlFrame = CGRect(x: 0, y: bar.minY - 30, width: size.width, height: 26)
        gridFrame = CGRect(
            x: 0,
            y: header.maxY + 18,
            width: size.width,
            height: pageControlFrame.minY - header.maxY - 18
        )

        // 4 columns on a phone, more on something wider.
        let margin: CGFloat = 12
        columns = max(Int(((size.width - margin * 2) / 100).rounded(.down)), 4)
        columnWidth = (size.width - margin * 2) / CGFloat(columns)
        iconSize = min(columnWidth * 0.76, 84).rounded()
        labelWidth = columnWidth - 4
        labelMaxWidth = (labelWidth * 1.19).rounded()
        rowHeight = iconSize + 4 + labelLineHeight + 14
        let lastRowHeight = iconSize + 4 + labelLineHeight
        rows = max(Int(((gridFrame.height - lastRowHeight) / rowHeight).rounded(.down)) + 1, 1)
    }

    var appsPerPage: Int { columns * rows }

    func iconFrame(slot: Int) -> CGRect {
        let col = CGFloat(slot % columns)
        let row = CGFloat(slot / columns)
        return CGRect(
            x: (12 + col * columnWidth + (columnWidth - iconSize) / 2).rounded(),
            y: (gridFrame.minY + row * rowHeight).rounded(),
            width: iconSize,
            height: iconSize
        )
    }

    func buttonFrames(count: Int, center: CGFloat, width: CGFloat) -> [CGRect] {
        let margin: CGFloat = 10
        let buttonWidth = (width - margin) / CGFloat(count) - margin
        return (0..<count).map { i in
            CGRect(
                x: margin + CGFloat(i) * (buttonWidth + margin),
                y: center - 15,
                width: buttonWidth,
                height: 30
            )
        }
    }
}

/// Works out each name's width and text, letting a name grow past
/// `labelWidth` only while it can't touch the names either side of it.
func layOutLabels(
    names: [String],
    layout: PickerLayout,
    font: UIFont
) -> [(text: String, width: CGFloat)] {
    func measure(_ text: String) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }
    let columns = layout.columns
    let textWidths = names.map { min(measure($0), layout.labelMaxWidth) }
    return names.indices.map { i -> (text: String, width: CGFloat) in
        var width = textWidths[i]
        if i % columns > 0 {
            width = min(width, 2 * layout.labelWidth - textWidths[i - 1])
        }
        if i % columns + 1 < columns && i + 1 < names.count {
            width = min(width, 2 * layout.labelWidth - textWidths[i + 1])
        }
        width = max(width, layout.labelWidth).rounded(.up)

        let name = names[i]
        guard measure(name) > width else {
            return (name, width)
        }
        // Like iOS, end names that still don't fit with an ellipsis.
        let chars = Array(name)
        var truncated = "…"
        for n in stride(from: chars.count - 1, through: 0, by: -1) {
            let candidate = String(chars[..<n]).trimmingCharacters(in: .whitespaces) + "…"
            if measure(candidate) <= width {
                truncated = candidate
                break
            }
        }
        // Rather than cut into the first word, drop the words after it.
        let firstWord = String(name.split(separator: " ", omittingEmptySubsequences: false)[0])
        if truncated.count - 1 < firstWord.count && measure(firstWord) <= width {
            truncated = firstWord
        }
        return (truncated, width)
    }
}

// MARK: - Views

struct PickerView: View {
    @ObservedObject var model: PickerModel

    var body: some View {
        content
            .statusBarHidden(false)
            // Back from File Manager (or anywhere else), maybe with games
            // shared with "Open in iDared 32bit".
            .onReceive(
                NotificationCenter.default
                    .publisher(for: UIApplication.didBecomeActiveNotification)
            ) { _ in
                model.importSharedGames()
            }
            .onReceive(
                NotificationCenter.default.publisher(for: OpenedGames.notification)
            ) { _ in
                model.importSharedGames()
            }
            .onAppear {
                model.importSharedGames()
            }
            .confirmationDialog(
                importConflictTitle,
                isPresented: Binding(
                    get: { !model.importConflicts.isEmpty },
                    set: { if !$0 { model.importConflicts = [] } }
                ),
                titleVisibility: .visible
            ) {
                Button("Replace") {
                    model.importConflicts = []
                    model.finishImport(replacing: true)
                }
                Button("Keep Existing") {
                    model.importConflicts = []
                    model.finishImport(replacing: false)
                }
                Button("Cancel", role: .cancel) {
                    model.importConflicts = []
                    model.cancelImport()
                }
            } message: {
                Text(model.importConflicts.joined(separator: "\n"))
            }
            .alert(
                "Couldn't Import",
                isPresented: Binding(
                    get: { model.importError != nil },
                    set: { if !$0 { model.importError = nil } }
                )
            ) {
                // Show any games that did import.
                Button("OK", role: .cancel) {
                    model.reloadIfGamesChanged()
                }
            } message: {
                Text(model.importError ?? "")
            }
            .sheet(isPresented: $model.showingSettings) {
                SettingsView(model: model)
            }
            .alert("The Game Stopped", isPresented: $model.showingStartError) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(model.input.startError ?? "")
            }
    }

    private var importConflictTitle: String {
        model.importConflicts.count == 1
            ? "This game is already in Games"
            : "These games are already in Games"
    }

    private var content: some View {
        GeometryReader { geometry in
            let layout = PickerLayout(
                size: geometry.size,
                insets: model.safeInsets,
                hideLogo: model.hideHeaderLogo
            )
            ZStack(alignment: .topLeading) {
                Color.black
                if let wallpaper = model.wallpaper {
                    // Below the header, which would hide its top.
                    let area = CGRect(
                        x: 0,
                        y: layout.header.maxY,
                        width: layout.size.width,
                        height: layout.size.height - layout.header.maxY
                    )
                    Image(uiImage: wallpaper)
                        .resizable()
                        .scaledToFill()
                        .frame(width: area.width, height: area.height)
                        .clipped()
                        .opacity(0.8)
                        .position(x: area.midX, y: area.midY)
                }
                if model.footerBackground {
                    // From the page dots, where the grid ends, to the
                    // bottom edge. Behind the grid, which draws the dots.
                    let top = layout.pageControlFrame.minY
                    model.headerColor.color
                        .place(CGRect(
                            x: 0,
                            y: top,
                            width: layout.size.width,
                            height: layout.size.height - top
                        ))
                }
                BrandingLabels(branding: model.input.branding, size: layout.size)
                if let error = model.input.error {
                    VStack(spacing: 20) {
                        Text(error)
                            .font(.system(size: 17))
                            .foregroundColor(.white)
                            .shadow(color: .black.opacity(0.6), radius: 2, y: 1)
                            .multilineTextAlignment(.center)
                        Button {
                            model.showingImporter = true
                        } label: {
                            Label("Import Games", systemImage: "square.and.arrow.down")
                                .font(.system(size: 17, weight: .semibold))
                                .padding(.horizontal, 20)
                                .padding(.vertical, 12)
                                .background(Capsule().fill(Color.white))
                                .foregroundColor(.black)
                        }
                        .buttonStyle(.plain)
                    }
                    .place(layout.gridFrame.insetBy(dx: 10, dy: 10))
                } else {
                    IconGrid(model: model, layout: layout)
                }
                Header(model: model, layout: layout)
                ActionBar(model: model)
                    .place(layout.bar)
                if model.showingCopyright {
                    CopyrightView(model: model, layout: layout)
                }
            }
        }
        .ignoresSafeArea()
        // Here, not beside the Settings sheet: on iOS 16, two presentations
        // on one view can keep each other from showing.
        .fileImporter(
            isPresented: $model.showingImporter,
            allowedContentTypes: [UTType(filenameExtension: "ipa") ?? .data, .folder],
            allowsMultipleSelection: true
        ) { result in
            if case .success(let urls) = result {
                model.importPicked(urls)
            }
        }
    }
}

struct Header: View {
    @ObservedObject var model: PickerModel
    let layout: PickerLayout

    var body: some View {
        let content = layout.headerContent
        let side: CGFloat = 124
        ZStack(alignment: .topLeading) {
            model.headerColor.color
                .place(layout.header)
            Text("v" + model.input.shortVersionText)
                .font(.system(size: 17, weight: .semibold))
                .foregroundColor(model.headerTextColor.color)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .frame(maxWidth: .infinity, alignment: .leading)
                .place(CGRect(x: 16, y: content.minY, width: 60, height: content.height))
            // The logo, or the name, is centred, clear of the version and
            // the About button.
            let logoFrame = CGRect(
                x: 80,
                y: content.minY + 2,
                width: content.maxX - 160,
                height: content.height - 4
            )
            if model.hideHeaderLogo {
                Text("iDared 32bit")
                    .font(.system(size: 24, weight: .black))
                    .fontWidth(.condensed)
                    .foregroundColor(model.headerTextColor.color)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                    .place(logoFrame)
            } else {
                Image("AppPickerLogo")
                    .resizable()
                    .scaledToFit()
                    .grayscale(model.blackHeaderShown ? 1 : 0)
                    .opacity(model.blackHeaderShown ? 0.65 : 1)
                    .place(logoFrame)
            }
            HStack(spacing: 8) {
                HeaderButton(
                    title: "About",
                    symbol: "info",
                    color: model.headerTextColor.color,
                    fill: .clear,
                    action: {
                        model.copyrightPage = 0
                        model.showingCopyright = true
                    }
                )
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
            .place(CGRect(
                x: content.maxX - side,
                y: content.minY,
                width: side - 12,
                height: content.height
            ))
        }
    }
}

struct HeaderButton: View {
    let title: String
    let symbol: String
    let color: Color
    let fill: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 4) {
                ZStack {
                    Circle().fill(fill)
                    Circle().stroke(color, lineWidth: 2)
                    Image(systemName: symbol)
                        .font(.system(size: 18, weight: .bold))
                        .foregroundColor(color)
                }
                .frame(width: 40, height: 40)
                Text(title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(color)
                    .lineLimit(1)
                    .fixedSize()
            }
            .frame(width: 52)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Import, File Manager and Settings, in one bar like the iOS 26
/// tab bar. A real toolbar's height is fixed too short for the labels.
struct ActionBar: View {
    @ObservedObject var model: PickerModel

    var body: some View {
        HStack(spacing: 0) {
            item("square.and.arrow.down.fill", "Import") {
                model.showingImporter = true
            }
            item("folder.fill", "File Manager") {
                model.open(model.input.fileManagerURL)
            }
            item("gearshape.fill", "Settings") {
                model.showingSettings = true
            }
        }
        .padding(.horizontal, 10)
        .modifier(BarBackground())
        .environment(\.colorScheme, .dark)
    }

    private func item(
        _ symbol: String,
        _ title: String,
        action: @escaping () -> Void
    ) -> some View {
        // Plain, or iOS underlines the titles when Button Shapes is on.
        Button(action: action) {
            VStack(spacing: 4) {
                Image(systemName: symbol)
                    .font(.system(size: 22))
                Text(title)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .foregroundColor(.white)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Liquid Glass on iOS 26, the translucent bar material before that.
struct BarBackground: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content.glassEffect(.regular.interactive(), in: Capsule())
        } else {
            content
                .background(.ultraThinMaterial, in: Capsule())
                .overlay(Capsule().stroke(Color.white.opacity(0.2), lineWidth: 0.5))
        }
    }
}

struct BrandingLabels: View {
    let branding: String
    let size: CGSize

    var body: some View {
        if !branding.isEmpty {
            ForEach(1...7, id: \.self) { i in
                Text(branding)
                    .font(.system(size: 48))
                    .foregroundColor(branding == "UNOFFICIAL" ? .red : Color(white: 0.5))
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: i % 2 == 0 ? .leading : .trailing)
                    .place(CGRect(
                        x: 0,
                        y: size.height / 8 * CGFloat(i) - 25,
                        width: size.width,
                        height: 50
                    ))
            }
        }
    }
}

struct IconGrid: View {
    @ObservedObject var model: PickerModel
    let layout: PickerLayout
    @State private var dragStart: CGPoint?

    private var pageCount: Int {
        (model.input.apps.count + layout.appsPerPage - 1) / layout.appsPerPage
    }

    private var pageRange: Range<Int> {
        let page = min(model.page, pageCount - 1)
        let start = page * layout.appsPerPage
        return start..<min(start + layout.appsPerPage, model.input.apps.count)
    }

    var body: some View {
        let range = pageRange
        let font = UIFont.systemFont(ofSize: labelFontSize, weight: .medium)
        // Empty slots still count as neighbours, with no name.
        let names = (0..<layout.appsPerPage).map { slot in
            range.contains(range.lowerBound + slot) ? model.input.apps[range.lowerBound + slot].name : ""
        }
        let labels = layOutLabels(names: names, layout: layout, font: font)

        ZStack(alignment: .topLeading) {
            Color.clear
            ForEach(Array(range.enumerated()), id: \.element) { slot, app in
                let iconFrame = layout.iconFrame(slot: slot)
                Image(uiImage: model.icons[app])
                    .resizable()
                    .opacity(model.pressedApp == app ? 0.5 : 1)
                    .shadow(color: .black.opacity(0.35), radius: 3, y: 2)
                    .place(iconFrame)
                if !model.hideIconLabels {
                    Text(labels[slot].text)
                        .font(.system(size: labelFontSize, weight: .medium))
                        .foregroundColor(.white)
                        .shadow(color: .black.opacity(0.7), radius: 2, y: 1)
                        .lineLimit(1)
                        .fixedSize()
                        .place(CGRect(
                            x: (iconFrame.midX - labels[slot].width / 2).rounded(),
                            y: (iconFrame.maxY + 4).rounded(),
                            width: labels[slot].width,
                            height: labelLineHeight
                        ))
                }
            }
            PageControl(model: model, pageCount: pageCount)
                .place(layout.pageControlFrame)
        }
        .frame(width: layout.size.width, height: layout.size.height)
        .contentShape(Rectangle())
        .gesture(DragGesture(minimumDistance: 0).onChanged(touchMoved).onEnded(touchEnded))
    }

    private func app(at point: CGPoint) -> Int? {
        let range = pageRange
        for (slot, app) in range.enumerated() where layout.iconFrame(slot: slot).contains(point) {
            return app
        }
        return nil
    }

    private func touchMoved(_ value: DragGesture.Value) {
        if dragStart == nil {
            // Only touches that start in the grid, not on the page control.
            guard layout.gridFrame.contains(value.startLocation),
                  !layout.pageControlFrame.contains(value.startLocation)
            else {
                return
            }
            dragStart = value.startLocation
            model.pressedApp = app(at: value.startLocation)
        }
        if abs(value.location.x - value.startLocation.x) > 50 {
            model.pressedApp = nil
        }
    }

    private func touchEnded(_ value: DragGesture.Value) {
        defer {
            dragStart = nil
        }
        guard dragStart != nil else {
            return
        }
        let dx = value.location.x - value.startLocation.x
        let pressed = model.pressedApp
        if dx > 50 {
            model.pressedApp = nil
            model.page = max(model.page - 1, 0)
        } else if dx < -50 {
            model.pressedApp = nil
            model.page = min(model.page + 1, pageCount - 1)
        } else if let pressed, app(at: value.location) == pressed {
            // Stays highlighted while the app starts.
            model.pick(pressed)
        } else {
            model.pressedApp = nil
        }
    }
}

struct PageControl: View {
    @ObservedObject var model: PickerModel
    let pageCount: Int
    private let dotSpacing: CGFloat = 18

    var body: some View {
        // Like UIPageControl's hidesForSinglePage: one dot says nothing.
        if pageCount > 1 {
            GeometryReader { geometry in
                let startX = geometry.size.width / 2 - CGFloat(pageCount - 1) * dotSpacing / 2
                ZStack(alignment: .topLeading) {
                    Color.clear
                    ForEach(0..<pageCount, id: \.self) { i in
                        let current = i == model.page
                        Circle()
                            .fill(Color.white.opacity(current ? 1 : 0.5))
                            .frame(width: current ? 9 : 7, height: current ? 9 : 7)
                            .position(
                                x: (startX + CGFloat(i) * dotSpacing).rounded(),
                                y: (geometry.size.height / 2).rounded()
                            )
                    }
                }
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onEnded { value in
                    tapped(at: value.location.x, startX: startX)
                })
            }
        }
    }

    // A tap on a dot goes to its page. The dots are too small to hit
    // reliably, so like UIPageControl, a tap anywhere else goes one page
    // back or forward, depending on which side of the current dot it's on.
    private func tapped(at x: CGFloat, startX: CGFloat) {
        let nearest = ((x - startX) / dotSpacing).rounded()
        if nearest >= 0 && Int(nearest) < pageCount
            && abs(x - (startX + nearest * dotSpacing)) <= dotSpacing / 2 {
            model.page = Int(nearest)
        } else if x < startX + CGFloat(model.page) * dotSpacing {
            model.page = max(model.page - 1, 0)
        } else {
            model.page = min(model.page + 1, pageCount - 1)
        }
    }
}

/// White rounded rectangles, like UIButtonTypeRoundedRect.
struct ButtonRow: View {
    let titles: [String]
    let frames: [CGRect]
    let actions: [() -> Void]
    var fontSize: CGFloat = 17
    var colors: [Color]? = nil
    var hidden: [Bool]? = nil
    var textColor: Color = .black

    var body: some View {
        ForEach(titles.indices, id: \.self) { i in
            if !(hidden?[i] ?? false) {
                Button(action: actions[i]) {
                    Text(titles[i])
                        .font(.system(size: fontSize))
                        .foregroundColor(textColor)
                        .lineLimit(1)
                        .frame(width: frames[i].width, height: frames[i].height)
                        .background(colors?[i] ?? .white)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                }
                .buttonStyle(.plain)
                .place(frames[i])
            }
        }
    }
}

/// A standard iOS settings sheet. The options apply to the next app opened.
/// Settings: the game options, with Theme Options on its own page.
struct SettingsView: View {
    @ObservedObject var model: PickerModel

    var body: some View {
        NavigationStack {
            Form {
                // Game options change every launch, so they come first;
                // themes are set once, so they're a tap away.
                ForEach(model.input.quickOptions.indices, id: \.self) { i in
                    section(i)
                }
                Section {
                    NavigationLink {
                        page("Theme Options") {
                            headerColorSection
                            headerTextColorSection
                            Section {
                                Toggle("Footer background color", isOn: $model.footerBackground)
                            } footer: {
                                Text("Apply the header background color to the footer too.")
                            }
                            Section {
                                Toggle("Hide header logo", isOn: $model.hideHeaderLogo)
                            } footer: {
                                Text("Shows the name instead, with a shorter header.")
                            }
                            wallpaperSection
                            Section {
                                Toggle("Hide icon label", isOn: $model.hideIconLabels)
                            } footer: {
                                Text("Shows the icons without the games' names under them.")
                            }
                        }
                    } label: {
                        Label("Theme Options", systemImage: "paintpalette")
                    }
                    // The same screen as the header's About button, which is
                    // drawn over the picker, so Settings closes first.
                    Button {
                        model.showingSettings = false
                        model.copyrightPage = 0
                        model.showingCopyright = true
                    } label: {
                        // White text like the row above; only the icon is
                        // tinted, as a button's whole label would be.
                        Label {
                            Text("About/Copyright")
                                .foregroundColor(.primary)
                        } icon: {
                            Image(systemName: "info.circle")
                        }
                    }
                    Button {
                        model.open(model.input.websiteURL)
                    } label: {
                        Label {
                            Text("iDared 32bit Website")
                                .foregroundColor(.primary)
                        } icon: {
                            Image(systemName: "safari")
                        }
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { doneButton }
        }
        .preferredColorScheme(.dark)
    }

    private func page(
        _ title: String,
        @ViewBuilder content: () -> some View
    ) -> some View {
        Form(content: content)
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { doneButton }
    }

    private var doneButton: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            // Since iOS 26, Done is a checkmark in a glass circle.
            if #available(iOS 26, *) {
                Button(role: .confirm) {
                    model.showingSettings = false
                }
            } else {
                Button("Done") {
                    model.showingSettings = false
                }
            }
        }
    }

    private var headerColorSection: some View {
        Section {
            // Seven fit across the narrowest iPhones at this size.
            HStack(spacing: 10) {
                ForEach(HeaderColor.allCases, id: \.self) { choice in
                    swatch(choice.color, choice.name, selected: model.headerColor == choice) {
                        model.headerColor = choice
                    }
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
        } header: {
            Text("Header background color")
        }
    }

    private var wallpaperSection: some View {
        Section {
            // Only used to hear what was picked.
            let picked = Binding<PhotosPickerItem?>(
                get: { nil },
                set: { item in
                    if let item {
                        loadWallpaper(item)
                    }
                }
            )
            PhotosPicker(selection: picked, matching: .images) {
                Text("Choose wallpaper")
            }
            if model.wallpaper != nil {
                Button("Remove wallpaper", role: .destructive) {
                    model.removeWallpaper()
                }
            }
        } header: {
            Text("Wallpaper")
        } footer: {
            Text("A photo to show behind the games' icons.")
        }
    }

    private func loadWallpaper(_ item: PhotosPickerItem) {
        Task { @MainActor in
            guard let data = try? await item.loadTransferable(type: Data.self),
                  let photo = UIImage(data: data)
            else {
                return
            }
            model.setWallpaper(photo)
        }
    }

    private var headerTextColorSection: some View {
        Section {
            HStack(spacing: 10) {
                ForEach(HeaderTextColor.allCases, id: \.self) { choice in
                    swatch(choice.color, choice.name, selected: model.headerTextColor == choice) {
                        model.headerTextColor = choice
                    }
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
        } header: {
            Text("Header text color")
        }
    }

    private func swatch(
        _ color: Color,
        _ name: String,
        selected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Circle()
                .fill(color)
                .frame(width: 34, height: 34)
                .overlay(Circle().stroke(Color.white.opacity(0.3), lineWidth: 1))
                .overlay(
                    Circle()
                        .stroke(Color.white, lineWidth: 2.5)
                        .padding(-4)
                        .opacity(selected ? 1 : 0)
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(name)
    }

    @ViewBuilder
    private func section(_ i: Int) -> some View {
        let option = model.input.quickOptions[i]
        let value = $model.quickOptionValues[i]
        if let choices = option.choices {
            Section {
                Picker(option.label, selection: value) {
                    ForEach(choices.indices, id: \.self) { j in
                        Text(choices[j]).tag(j)
                    }
                }
                .modifier(ChoiceStyle(choices: choices))
                .labelsHidden()
            } header: {
                Text(option.label)
            } footer: {
                footer(option)
            }
        } else {
            Section {
                Toggle(option.label, isOn: Binding(
                    get: { value.wrappedValue != 0 },
                    set: { value.wrappedValue = $0 ? 1 : 0 }
                ))
            } footer: {
                footer(option)
            }
        }
    }

    @ViewBuilder
    private func footer(_ option: PickerInput.QuickOption) -> some View {
        if !option.footer.isEmpty {
            Text(option.footer)
        }
    }
}

/// Short choices fit side by side; longer ones are a list with a checkmark.
struct ChoiceStyle: ViewModifier {
    let choices: [String]

    func body(content: Content) -> some View {
        if choices.allSatisfy({ $0.count <= 7 }) {
            content.pickerStyle(.segmented)
        } else {
            content.pickerStyle(.inline)
        }
    }
}

struct CopyrightView: View {
    @ObservedObject var model: PickerModel
    let layout: PickerLayout

    private static let font = UIFont.systemFont(ofSize: 16)

    var body: some View {
        // The background covers the whole screen, but the contents stay in
        // the safe area, clear of the Dynamic Island and home indicator.
        let insets = layout.insets
        let safeHeight = layout.size.height - insets.top - insets.bottom
        let width = layout.size.width
        let divider = safeHeight - 40
        let textFrame = CGRect(x: 10, y: insets.top + 10, width: width - 20, height: divider - 20)
        let pages = copyrightPages(for: textFrame.size)
        let page = min(model.copyrightPage, pages.count - 1)

        ZStack(alignment: .topLeading) {
            Color.black
            Text(pages[page])
                .font(.system(size: 16))
                .foregroundColor(.white)
                .frame(width: textFrame.width, height: textFrame.height, alignment: .topLeading)
                .place(textFrame)
            ButtonRow(
                titles: ["↑", "↓", "×"],
                frames: layout.buttonFrames(
                    count: 3,
                    center: insets.top + (safeHeight + divider) / 2,
                    width: width
                ),
                actions: [
                    { model.copyrightPage = max(page - 1, 0) },
                    { model.copyrightPage = min(page + 1, pages.count - 1) },
                    { model.showingCopyright = false },
                ],
                fontSize: 30,
                colors: [.clear, .clear, .clear],
                hidden: [page == 0, page == pages.count - 1, false],
                textColor: .white
            )
        }
        .frame(width: layout.size.width, height: layout.size.height)
    }

    private func copyrightPages(for size: CGSize) -> [String] {
        if model.copyrightPages.isEmpty {
            model.copyrightPages = paginate(model.input.copyright, size: size)
        }
        return model.copyrightPages
    }

    /// Splits the text into pages that fit, breaking before "###" headings
    /// and after "---" dividers.
    private func paginate(_ text: String, size: CGSize) -> [String] {
        func height(_ line: Substring) -> CGFloat {
            let line = line.hasSuffix("\n") ? line.dropLast() : line
            if line.isEmpty {
                return Self.font.lineHeight
            }
            return (String(line) as NSString).boundingRect(
                with: CGSize(width: size.width, height: .greatestFiniteMagnitude),
                options: .usesLineFragmentOrigin,
                attributes: [.font: Self.font],
                context: nil
            ).height.rounded(.up)
        }

        var lines: [Substring] = []
        var lineStart = text.startIndex
        while lineStart < text.endIndex {
            let lineEnd = text[lineStart...].firstIndex(of: "\n").map { text.index(after: $0) }
                ?? text.endIndex
            lines.append(text[lineStart..<lineEnd])
            lineStart = lineEnd
        }

        var pages: [String] = []
        var i = 0
        while i < lines.count {
            // Pages don't start with blank lines.
            while i < lines.count && lines[i].trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                i += 1
            }
            var page = ""
            var pageHeight: CGFloat = 0
            let pageStart = i
            while i < lines.count {
                let line = lines[i]
                let isFirstLine = i == pageStart
                if !isFirstLine && line.hasPrefix("###") {
                    break
                }
                let lineHeight = height(line)
                if !isFirstLine && pageHeight + lineHeight > size.height {
                    break
                }
                page += line
                pageHeight += lineHeight
                i += 1
                if !isFirstLine && line.hasPrefix("---") {
                    break
                }
            }
            if !page.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                pages.append(page)
            }
        }
        return pages.isEmpty ? [""] : pages
    }
}
