/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// Plays games' movies full screen with AVKit, for
// src/frameworks/media_player/movie_player.rs.

import AVKit
import UIKit

private enum Movie {
    nonisolated(unsafe) static var controller: MovieViewController?
    nonisolated(unsafe) static var window: UIWindow?
    nonisolated(unsafe) static var endObserver: NSObjectProtocol?
    // TODO: remove once SpyMouse HD's black movie is understood.
    nonisolated(unsafe) static var statusObserver: NSKeyValueObservation?
}

/// Holds the player, in its own window over SDL's, which SDL can't cover
/// when it swaps its GL views. It's turned to the orientation the game
/// asked for: some games' movies are made sideways for iPhone OS 3's
/// landscape player. The interface itself keeps the game's orientations,
/// the only ones iOS allows while it's running, so the player is turned
/// within it instead.
private final class MovieViewController: UIViewController {
    let player = AVPlayerViewController()
    let content = UIView()
    /// `.unknown` for the game's own.
    var orientation = UIInterfaceOrientation.unknown
    /// The game's orientations (SDL's view controller's).
    var allowed = UIInterfaceOrientationMask.all
    var controls = true

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        view.addSubview(content)
        addChild(player)
        player.view.frame = content.bounds
        player.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        content.addSubview(player.view)
        player.didMove(toParent: self)
        player.showsPlaybackControls = controls
        guard controls else {
            return
        }
        // AVKit only has its own close button when presented by itself.
        let close = UIButton(type: .close, primaryAction: UIAction { _ in
            idaredMovieStop()
        })
        close.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(close)
        NSLayoutConstraint.activate([
            close.leadingAnchor.constraint(
                equalTo: content.leadingAnchor,
                constant: 24,
            ),
            close.topAnchor.constraint(equalTo: content.topAnchor, constant: 24),
        ])
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let current = view.window?.windowScene?.effectiveGeometry
            .interfaceOrientation ?? .unknown
        let turn = orientation == .unknown || current == .unknown
            ? 0 : angle(orientation) - angle(current)
        var size = view.bounds.size
        if Int((abs(turn) / (.pi / 2)).rounded()) % 2 == 1 {
            size = CGSize(width: size.height, height: size.width)
        }
        content.transform = .identity
        content.bounds = CGRect(origin: .zero, size: size)
        content.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        content.transform = CGAffineTransform(rotationAngle: turn)
    }

    /// How far an orientation's content is turned clockwise from portrait.
    private func angle(_ orientation: UIInterfaceOrientation) -> CGFloat {
        switch orientation {
        case .landscapeRight: .pi / 2
        case .portraitUpsideDown: .pi
        case .landscapeLeft: -.pi / 2
        default: 0
        }
    }

    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        allowed
    }
    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
}

@_cdecl("idared_movie_play")
public func idaredMoviePlay(
    _ path: UnsafePointer<CChar>,
    _ controls: Bool,
    _ orientation: Int32,
) {
    idaredMovieStop()
    guard let presenter = topViewController() else {
        return
    }
    let item = AVPlayerItem(url: URL(fileURLWithPath: String(cString: path)))
    let controller = MovieViewController()
    controller.player.player = AVPlayer(playerItem: item)
    controller.controls = controls
    controller.orientation =
        UIInterfaceOrientation(rawValue: Int(orientation)) ?? .unknown
    controller.allowed = presenter.supportedInterfaceOrientations
    Movie.endObserver = NotificationCenter.default.addObserver(
        forName: .AVPlayerItemDidPlayToEndTime,
        object: item,
        queue: .main,
    ) { _ in
        idaredMovieStop()
    }
    Movie.statusObserver = item.observe(\.status) { item, _ in
        print(
            "Movie: status \(item.status.rawValue),",
            "size \(item.presentationSize),",
            "error \(String(describing: item.error))",
        )
    }
    Movie.controller = controller
    let window = presenter.view.window?.windowScene.map {
        UIWindow(windowScene: $0)
    } ?? UIWindow(frame: UIScreen.main.bounds)
    window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue + 1)
    window.rootViewController = controller
    window.isHidden = false
    Movie.window = window
    print(
        "Movie: window over \(String(describing: presenter.view.window)),",
        "frame \(controller.view.frame),",
        "player \(controller.player.view.frame)",
    )
    controller.player.player?.play()
}

/// Whether the movie has ended, been closed by the user, or been stopped.
@_cdecl("idared_movie_finished")
public func idaredMovieFinished() -> Bool {
    Movie.controller == nil
}

@_cdecl("idared_movie_stop")
public func idaredMovieStop() {
    if let observer = Movie.endObserver {
        NotificationCenter.default.removeObserver(observer)
        Movie.endObserver = nil
    }
    Movie.statusObserver = nil
    guard let controller = Movie.controller else {
        return
    }
    Movie.controller = nil
    controller.player.player?.pause()
    Movie.window?.isHidden = true
    Movie.window?.rootViewController = nil
    Movie.window = nil
}

/// SDL's view controller, or whatever it's presenting.
private func topViewController() -> UIViewController? {
    let windows = UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap { $0.windows }
    // Not SDL's launch screen or another window that's key.
    let sdl: AnyClass? = NSClassFromString("SDL_uikitviewcontroller")
    var top = windows.lazy.compactMap { $0.rootViewController }
        .first { sdl != nil && $0.isKind(of: sdl!) }
        ?? (windows.first { $0.isKeyWindow } ?? windows.first)?
        .rootViewController
    while let presented = top?.presentedViewController {
        top = presented
    }
    return top
}
