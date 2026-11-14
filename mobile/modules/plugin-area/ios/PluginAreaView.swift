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
    controller?.view.frame = bounds
  }

  private func attach() {
    guard let areaId, let area = Areas[areaId], attached !== area else { return }
    guard let parent = nearestViewController() else { return }
    detach()
    area.view?.detach()
    let flutter = FlutterViewController(engine: area.engine, nibName: nil, bundle: nil)
    flutter.isViewOpaque = false
    flutter.view.backgroundColor = .clear
    parent.addChild(flutter)
    flutter.view.frame = bounds
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
