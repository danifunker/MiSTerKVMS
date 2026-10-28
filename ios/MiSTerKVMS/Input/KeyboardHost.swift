import SwiftUI
import UIKit

/// Receives the iOS system keyboard (P10). It sends characters, not keys:
/// the core types them as key presses on a US layout. The function bar sits
/// on top of the keyboard as its accessory view; with the keyboard hidden
/// and the bar shown, an empty input view leaves just the bar.
final class KeyInputView: UIView, UIKeyInput {
    var onText: ((String) -> Void)?
    var onBackspace: (() -> Void)?
    /// A hardware keyboard's key went down or up (USB HID usage). Keys
    /// taken here never reach `insertText`.
    var onKey: ((UInt16, Bool) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        // No shortcuts bar on an iPad with a hardware keyboard.
        inputAssistantItem.leadingBarButtonGroups = []
        inputAssistantItem.trailingBarButtonGroups = []
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = forward(presses, down: true)
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = forward(presses, down: false)
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = forward(presses, down: false)
        if !rest.isEmpty { super.pressesCancelled(rest, with: event) }
    }

    /// Sends the keyboard's keys on; returns the presses that aren't keys.
    private func forward(_ presses: Set<UIPress>, down: Bool) -> Set<UIPress> {
        guard let onKey else { return presses }
        var rest = Set<UIPress>()
        for press in presses {
            if let key = press.key {
                onKey(UInt16(truncatingIfNeeded: key.keyCode.rawValue), down)
            } else {
                rest.insert(press)
            }
        }
        return rest
    }

    var systemKeyboard = true {
        didSet { if systemKeyboard != oldValue, isFirstResponder { reloadInputViews() } }
    }
    var bar: UIView? {
        didSet { if bar !== oldValue, isFirstResponder { reloadInputViews() } }
    }
    private let noKeyboard = UIView(frame: .zero)

    override var canBecomeFirstResponder: Bool { true }

    /// Keys held on a hardware keyboard would never see their release once
    /// the focus goes (a sheet opens): let them go on the MiSTer.
    @discardableResult override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned { onResign?() }
        return resigned
    }
    var onResign: (() -> Void)?
    override var inputView: UIView? { systemKeyboard ? nil : noKeyboard }
    override var inputAccessoryView: UIView? { bar }

    // Always "has text", so Backspace reaches us even with nothing typed.
    var hasText: Bool { true }
    func insertText(_ text: String) { onText?(text) }
    func deleteBackward() { onBackspace?() }

    // Plain keys: no corrections, capitals or smart punctuation.
    var autocorrectionType: UITextAutocorrectionType = .no
    var autocapitalizationType: UITextAutocapitalizationType = .none
    var spellCheckingType: UITextSpellCheckingType = .no
    var smartQuotesType: UITextSmartQuotesType = .no
    var smartDashesType: UITextSmartDashesType = .no
    var smartInsertDeleteType: UITextSmartInsertDeleteType = .no
    var inlinePredictionType: UITextInlinePredictionType = .no
    var keyboardType: UIKeyboardType = .asciiCapable
    var keyboardAppearance: UIKeyboardAppearance = .dark
    var returnKeyType: UIReturnKeyType = .default
}

struct KeyboardHost: UIViewRepresentable {
    let model: AppModel
    var showKeyboard: Bool
    var showFunctionBar: Bool
    /// Keeps the focus for a hardware keyboard with nothing on screen.
    var hardwareKeyboard: Bool
    let bar: FunctionBarState

    final class Coordinator {
        var bar: UIHostingController<AnyView>?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> KeyInputView {
        let view = KeyInputView(frame: .zero)
        view.onText = { [weak model] text in model?.type(text) }
        view.onBackspace = { [weak model] in model?.tap(HID.backspace) }
        view.onKey = { [weak model] usage, down in model?.key(usage, down: down) }
        view.onResign = { [weak model] in model?.releaseKeys() }
        return view
    }

    func updateUIView(_ view: KeyInputView, context: Context) {
        if showFunctionBar {
            if context.coordinator.bar == nil {
                let host = UIHostingController(rootView: AnyView(FunctionBar(state: bar).environmentObject(model)))
                host.view.frame = CGRect(x: 0, y: 0, width: 0, height: FunctionBar.height)
                host.view.autoresizingMask = .flexibleWidth
                host.view.backgroundColor = .clear
                context.coordinator.bar = host
            }
            view.bar = context.coordinator.bar?.view
        } else {
            view.bar = nil
        }
        view.systemKeyboard = showKeyboard
        let wanted = showKeyboard || showFunctionBar || hardwareKeyboard
        DispatchQueue.main.async {
            if wanted, !view.isFirstResponder, view.window != nil {
                view.becomeFirstResponder()
            } else if !wanted, view.isFirstResponder {
                view.resignFirstResponder()
            }
        }
    }
}

/// Shared between the session screen and the function bar (which lives in
/// the keyboard's window, so it can't read the screen's state itself).
@MainActor
final class FunctionBarState: ObservableObject {
    /// Where the bar's top edge is on the screen, nil while it isn't shown.
    /// With the keyboard hidden iOS reports neither a keyboard frame nor a
    /// keyboard safe area for the bar alone, so the bar reports itself and
    /// the quick buttons stay above it.
    @Published var top: CGFloat?
    /// The on-screen keyboard is up; nil hides the bar's keyboard key (a
    /// hardware keyboard is connected, so iOS won't show it).
    @Published var keyboardShown: Bool?
    var toggleKeyboard: () -> Void = {}
}

/// Keys the iOS keyboard doesn't have: Esc, Tab, modifiers (tap: next key
/// only; tap again: locked; again: off), F1–F12, Del; first, a key that
/// shows or hides the keyboard itself.
struct FunctionBar: View {
    static let height: CGFloat = 46
    @ObservedObject var state: FunctionBarState
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                if let shown = state.keyboardShown {
                    Button { state.toggleKeyboard() } label: {
                        Image(systemName: shown ? "keyboard.chevron.compact.down" : "keyboard")
                            .font(.system(size: 15, weight: .medium)).frame(minWidth: 34).padding(.vertical, 7).padding(.horizontal, 6)
                    }
                    .buttonStyle(KeyStyle(active: false))
                    .accessibilityLabel(shown ? "Hide the keyboard" : "Show the keyboard")
                    Divider().frame(height: 24)
                }
                key("Esc", HID.escape)
                key("Tab", HID.tab)
                modifier("Ctrl", HID.leftCtrl)
                modifier("Alt", HID.leftAlt)
                modifier("Shift", HID.leftShift)
                modifier("Win", HID.leftGUI)
                Divider().frame(height: 24)
                ForEach(HID.functionKeys, id: \.1) { name, usage in key(name, usage) }
                Divider().frame(height: 24)
                key("Del", HID.delete)
            }
            .padding(.horizontal, 8)
        }
        .frame(height: Self.height)
        .background(.ultraThinMaterial)
        // The keyboard's window covers the screen, so .global is the screen.
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minY } action: { state.top = $0 }
        .onDisappear { state.top = nil }
    }

    private func key(_ title: String, _ usage: UInt16) -> some View {
        Button { model.tap(usage) } label: {
            Text(title).font(.system(size: 15, weight: .medium)).frame(minWidth: 34).padding(.vertical, 7).padding(.horizontal, 6)
        }
        .buttonStyle(KeyStyle(active: false))
    }

    private func modifier(_ title: String, _ usage: UInt16) -> some View {
        let state = model.modifierState(usage)
        return Button { model.cycleModifier(usage) } label: {
            HStack(spacing: 3) {
                Text(title)
                if state == .locked { Image(systemName: "lock.fill").font(.system(size: 9)) }
            }
            .font(.system(size: 15, weight: .medium)).frame(minWidth: 34).padding(.vertical, 7).padding(.horizontal, 6)
        }
        .buttonStyle(KeyStyle(active: state != .off))
    }
}

struct KeyStyle: ButtonStyle {
    var active: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(active ? Color.black : Color.white)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(active ? Color.accentColor : Color.white.opacity(configuration.isPressed ? 0.35 : 0.15))
            )
    }
}
