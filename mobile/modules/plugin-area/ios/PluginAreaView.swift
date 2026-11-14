import ExpoModulesCore
import Flutter
import UIKit

/// Shows one area while mounted: its engine's FlutterViewController, a
/// child of the nearest view controller, transparent so React Native's
/// sheets and clips work over it. Removing it leaves the engine and its
/// Dart state; a second view of the same area takes it over.
class PluginAreaView: ExpoView {
  private var areaId: String?
  private var controller: FlutterViewController?
  private weak var attached: Area?

  func setAreaId(_ id: String) {
    if id == areaId { return }
    areaId = id
    if window != nil { attach() }
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window != nil { attach() } else { detach() }
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    if let flutter = controller { place(flutter.view) }
  }

  /// Sizes Flutter's view to whole device pixels. React Native lays out in
  /// floats, so an area meant to be 676.667 points tall at 3x is
  /// 676.66668701171875 points: 2030.00006 pixels. Flutter takes that as the
  /// view's exact size, draws a 2030-pixel frame and then discards it, since
  /// the engine checks the frame against the view's size with no tolerance
  /// (`Shell::ShouldDiscardLayerTree`): every frame is dropped and the area
  /// stays empty. Rounded to whole pixels, points times scale is exact again.
  private func place(_ view: UIView) {
    let scale = window?.windowScene?.screen.scale ?? traitCollection.displayScale
    let pixels = { (points: CGFloat) in (points * scale).rounded() / scale }
    view.frame = CGRect(x: 0, y: 0, width: pixels(bounds.width), height: pixels(bounds.height))
  }

  private func attach() {
    guard let areaId, let area = Areas[areaId], attached !== area else { return }
    guard let parent = nearestViewController() else { return }
    detach()
    area.view?.detach()
    let flutter = FlutterViewController(engine: area.engine, nibName: nil, bundle: nil)
    flutter.isViewOpaque = false
    flutter.view.backgroundColor = .clear
    // Only `place` sizes it, so UIKit adds no fractional resizes in between.
    flutter.view.autoresizingMask = []
    parent.addChild(flutter)
    place(flutter.view)
    addSubview(flutter.view)
    flutter.didMove(toParent: parent)
    controller = flutter
    area.view = self
    attached = area
  }

  func detach() {
    guard let flutter = controller else { return }
    flutter.willMove(toParent: nil)
    flutter.view.removeFromSuperview()
    flutter.removeFromParent()
    if let area = attached {
      if area.view === self { area.view = nil }
      area.engine.viewController = nil
    }
    controller = nil
    attached = nil
  }

  private func nearestViewController() -> UIViewController? {
    var responder: UIResponder? = self
    while let current = responder {
      if let found = current as? UIViewController { return found }
      responder = current.next
    }
    return nil
  }
}
