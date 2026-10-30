import AVFoundation
import AVKit
import ExpoModulesCore

// The call's sound output, as in FaceTime: its glyph is where the sound goes
// now (speaker, iPhone, AirPods, headphones, car), and a tap opens the
// system's route menu over it. The menu is AVRoutePickerView's, its own
// AirPlay glyph hidden under ours.
class AudioRouteButtonView: ExpoView {
  private let picker = AVRoutePickerView()
  private let glyph = UIImageView()
  private var routeName = ""
  private var symbol = ""
  /** The glyph's size, from the button's: as the app's own icons in a button that size. */
  private var pointSize: CGFloat = 0

  var tint: UIColor = .label {
    didSet { glyph.tintColor = tint }
  }

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    glyph.contentMode = .center
    glyph.tintColor = tint
    glyph.isUserInteractionEnabled = false
    picker.prioritizesVideoDevices = false
    picker.tintColor = .clear
    picker.activeTintColor = .clear
    addSubview(picker)
    addSubview(glyph)

    isAccessibilityElement = true
    accessibilityTraits = .button
    accessibilityLabel = "Audio output"

    NotificationCenter.default.addObserver(
      self,
      selector: #selector(routeChanged),
      name: AVAudioSession.routeChangeNotification,
      object: nil
    )
    showRoute()
  }

  deinit {
    NotificationCenter.default.removeObserver(self)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    picker.frame = bounds
    glyph.frame = bounds
    let size = (min(bounds.width, bounds.height) * 0.39).rounded()
    if size > 0 && size != pointSize {
      pointSize = size
      drawGlyph()
    }
  }

  // VoiceOver's double tap opens the menu as a tap would.
  override func accessibilityActivate() -> Bool {
    guard let button = picker.subviews.compactMap({ $0 as? UIButton }).first else { return false }
    button.sendActions(for: .touchUpInside)
    return true
  }

  @objc private func routeChanged() {
    DispatchQueue.main.async { [weak self] in self?.showRoute() }
  }

  private func showRoute() {
    let output = AVAudioSession.sharedInstance().currentRoute.outputs.first
    routeName = output?.portName ?? ""
    symbol = Self.symbol(for: output)
    accessibilityValue = routeName
    drawGlyph()
  }

  private func drawGlyph() {
    guard pointSize > 0, !symbol.isEmpty else { return }
    glyph.image = UIImage(
      systemName: symbol,
      withConfiguration: UIImage.SymbolConfiguration(pointSize: pointSize, weight: .regular)
    )
  }

  private static func symbol(for output: AVAudioSessionPortDescription?) -> String {
    guard let output else { return "speaker.wave.2.fill" }
    switch output.portType {
    case .builtInReceiver:
      return "iphone"
    case .builtInSpeaker:
      return "speaker.wave.2.fill"
    case .bluetoothA2DP, .bluetoothHFP, .bluetoothLE:
      let name = output.portName.lowercased()
      if name.contains("airpods max") { return "airpodsmax" }
      if name.contains("airpods pro") { return "airpodspro" }
      if name.contains("airpods") { return "airpods" }
      if name.contains("beats") { return "beats.headphones" }
      return "headphones"
    case .headphones, .usbAudio:
      return "headphones"
    case .carAudio:
      return "car.fill"
    case .airPlay:
      return "airplayaudio"
    default:
      return "speaker.wave.2.fill"
    }
  }
}
