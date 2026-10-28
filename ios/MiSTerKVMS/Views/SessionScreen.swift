import SwiftUI

/// One MiSTer, full screen (P13): the video as a touchpad, the keyboard and
/// function bar, the quick buttons, the control banner. Each can be hidden
/// from the controls menu (the corner button, or a three-finger tap), P14,
/// and the quick buttons can be moved anywhere (Arrange mode).
struct SessionScreen: View {
    @EnvironmentObject private var model: AppModel
    @AppStorage(Pref.keyboard) private var showKeyboard = true
    @AppStorage(Pref.functionBar) private var showFunctionBar = true
    @AppStorage(Pref.banner) private var showBanner = true
    @AppStorage(Pref.stats) private var showStats = false
    @State private var menuOpen = false
    @State private var switcherOpen = false
    @State private var arranging = false
    @StateObject private var barFrame = FunctionBarFrame()
    @StateObject private var hardware = HardwareInput()

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            TouchpadVideo(model: model, onMenu: { menuOpen.toggle() }, onSwitch: { model.step($0) },
                          onPointerOver: { hardware.pointerOverPicture = $0 })
                .ignoresSafeArea(.container)

            KeyboardHost(model: model, showKeyboard: showKeyboard && !sheetOpen, showFunctionBar: showFunctionBar && !sheetOpen,
                         hardwareKeyboard: hardware.keyboardConnected && !sheetOpen, barFrame: barFrame)
                .frame(width: 0, height: 0)

            statusOverlay

            // Above the function bar; without it, above the keyboard (its safe area).
            GeometryReader { geo in
                QuickButtons(arranging: $arranging)
                    .padding(.bottom, barFrame.top.map { max(0, geo.frame(in: .global).maxY - $0) } ?? 0)
            }
            .ignoresSafeArea(barFrame.top == nil ? [] : .keyboard)

            if arranging {
                arrangeBar
            } else {
                VStack(spacing: 6) {
                    HStack(alignment: .top) {
                        // The name gets the room first (tight on an iPhone Duo,
                        // whose status column takes the right edge).
                        misterButton.layoutPriority(1)
                        Spacer(minLength: 0)
                        if model.state.stream?.connected == true { controlButton }
                        Button { model.copyScreenshot() } label: {
                            Image(uiImage: .copyScreenshot)
                                .frame(width: 40, height: 40)
                                .background(Circle().fill(.black.opacity(0.5)))
                        }
                        .tint(.white)
                        // Never disabled: a disabled button lets the tap
                        // through to the touchpad, a click on the MiSTer.
                        .accessibilityLabel("Copy screenshot")
                        Button { model.muted.toggle() } label: {
                            Image(systemName: model.muted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                                .font(.system(size: 15))
                                .frame(width: 40, height: 40)
                                .background(Circle().fill(.black.opacity(0.5)))
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .tint(.white)
                        .accessibilityLabel(model.muted ? "Unmute" : "Mute")
                        Button { menuOpen = true } label: {
                            Image(systemName: "slider.horizontal.3")
                                .font(.system(size: 17))
                                .frame(width: 40, height: 40)
                                .background(Circle().fill(.black.opacity(0.5)))
                        }
                        .tint(.white)
                        .accessibilityLabel("Controls")
                        Button { model.close() } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 15, weight: .semibold))
                                .frame(width: 40, height: 40)
                                .background(Circle().fill(.black.opacity(0.5)))
                        }
                        .tint(.white)
                        .accessibilityLabel("Disconnect")
                    }
                    if showBanner, let banner = controlBanner {
                        HStack { banner; Spacer() }
                    }
                    if let notice = model.screenshotNotice {
                        Label { Text(notice) } icon: { Image(uiImage: .copyScreenshot) }
                            .font(.footnote)
                            .padding(.horizontal, 12).padding(.vertical, 6)
                            .background(Capsule().fill(.black.opacity(0.7)))
                            .transition(.opacity)
                            .allowsHitTesting(false)
                    }
                    if showStats { stats }
                    if model.isDemo {
                        HStack { DemoInputList(); Spacer() }
                    }
                    Spacer()
                }
                .padding(.horizontal, 12)
                .padding(.top, 4)
                .animation(.easeInOut(duration: 0.2), value: model.screenshotNotice)
            }
        }
        .onAppear { hardware.model = model }
        .onChange(of: sheetOpen || arranging, initial: true) { _, busy in hardware.enabled = !busy }
        .statusBarHidden()
        .persistentSystemOverlays(.hidden)
        .sheet(isPresented: $switcherOpen) {
            MiSTerSwitcher()
                .presentationDetents([.medium, .large])
                .presentationBackground(.ultraThinMaterial)
        }
        .sheet(isPresented: $menuOpen) {
            ControlsMenu(arrange: { arranging = true })
                .presentationDetents([.medium, .large])
                .presentationBackground(.ultraThinMaterial)
        }
    }

    private var sheetOpen: Bool { menuOpen || switcherOpen }

    /// The open MiSTer's name: tap to switch to another one or disconnect.
    private var misterButton: some View {
        Button { switcherOpen = true } label: {
            HStack(spacing: 6) {
                if model.state.stream?.internet == true {
                    Image(systemName: "globe").font(.system(size: 12, weight: .semibold)).accessibilityLabel("Over the internet")
                }
                Text(model.profile?.name ?? "MiSTer").font(.subheadline.weight(.semibold)).lineLimit(1)
                Image(systemName: "chevron.down").font(.system(size: 11, weight: .bold))
            }
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(Capsule().fill(.black.opacity(0.5)))
        }
        .tint(.white)
        .accessibilityLabel("Switch MiSTer or disconnect")
    }

    /// Arrange mode's instructions and Done.
    private var arrangeBar: some View {
        VStack {
            HStack(spacing: 12) {
                Text("Drag the buttons where you want them")
                    .font(.callout)
                Spacer()
                Button("Done") { arranging = false }
                    .buttonStyle(.borderedProminent)
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(RoundedRectangle(cornerRadius: 14).fill(.black.opacity(0.8)))
            .padding(.horizontal, 12)
            .padding(.top, 4)
            Spacer()
        }
    }

    /// Connecting, or why there's no picture.
    @ViewBuilder private var statusOverlay: some View {
        let text: String? = {
            switch model.state.connection {
            case .connecting: return "Connecting to \(model.profile?.name ?? "MiSTer")…"
            case .disconnected(let reason): return reason.isEmpty ? "Not connected" : reason
            case .connected:
                if let stream = model.state.stream, !stream.connected { return stream.error.isEmpty ? "Connecting to the capture server…" : stream.error }
                return model.state.videoError
            }
        }()
        if let text {
            Text(text)
                .font(.callout)
                .multilineTextAlignment(.center)
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.7)))
                .padding(.horizontal, 40)
                .allowsHitTesting(false)
        }
    }

    /// Filled: this phone controls the MiSTer, tap to give control up.
    /// Outline: watching, tap to take control (CAPTURE-SERVER.md §7).
    private var controlButton: some View {
        Button {
            model.inControl ? model.releaseControl() : model.takeControl()
        } label: {
            Image(systemName: model.inControl ? "gamecontroller.fill" : "gamecontroller")
                .font(.system(size: 15))
                .foregroundStyle(model.inControl ? Color.accentColor : .white)
                .frame(width: 40, height: 40)
                .background(Circle().fill(.black.opacity(0.5)))
                .contentTransition(.symbolEffect(.replace))
        }
        .tint(.white)
        .accessibilityLabel(model.inControl ? "Release control" : "Take control")
    }

    /// "Watching — Name has control" with Take control (CAPTURE-SERVER.md §7).
    private var controlBanner: AnyView? {
        guard let stream = model.state.stream, stream.connected else { return nil }
        if model.someoneElseInControl || model.watching {
            let who = stream.holder.isEmpty ? "Watching: nobody has control" : "\(stream.holder) has control"
            return AnyView(
                HStack(spacing: 8) {
                    Text(who).font(.footnote)
                    Button("Take control") { model.takeControl() }
                        .font(.footnote.weight(.semibold))
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                }
                .padding(.horizontal, 10).padding(.vertical, 6)
                .background(Capsule().fill(.black.opacity(0.7)))
            )
        }
        if !stream.misterConnected {
            return AnyView(
                Text("The capture server can't reach the MiSTer")
                    .font(.footnote)
                    .padding(.horizontal, 10).padding(.vertical, 6)
                    .background(Capsule().fill(.black.opacity(0.7)))
            )
        }
        return nil
    }

    private var stats: some View {
        let v = model.state.videoStats
        let parts: [String] = [
            v.map { "\($0.sourceWidth)×\($0.sourceHeight)" },
            v.map { String(format: "%.0f fps", $0.receivedFps) },
            v.map { String(format: "%.0f ms", $0.frameAgeMs) },
            model.state.stream.map { String(format: "rtt %.0f ms", $0.roundTripMs) },
        ].compactMap { $0 }
        return HStack {
            Text(parts.joined(separator: "  ·  "))
                .font(.caption.monospacedDigit())
                .padding(.horizontal, 8).padding(.vertical, 4)
                .background(Capsule().fill(.black.opacity(0.6)))
            Spacer()
        }
        .allowsHitTesting(false)
    }
}

extension UIImage {
    /// A camera on a clipboard (there's no such SF Symbol), tinted like one.
    static let copyScreenshot: UIImage = {
        let board = UIImage(systemName: "clipboard", withConfiguration: UIImage.SymbolConfiguration(pointSize: 17))
        let camera = UIImage(systemName: "camera.fill", withConfiguration: UIImage.SymbolConfiguration(pointSize: 17 * 0.42, weight: .semibold))
        guard let board, let camera else { return UIImage() }
        let image = UIGraphicsImageRenderer(size: board.size).image { _ in
            board.draw(at: .zero)
            let size = camera.size
            camera.draw(in: CGRect(x: (board.size.width - size.width) / 2, y: (board.size.height - size.height) / 2 + board.size.height * 0.06,
                                   width: size.width, height: size.height))
        }
        return image.withRenderingMode(.alwaysTemplate)
    }()
}

/// Show or hide each part of the session screen; back to the host list.
struct ControlsMenu: View {
    /// Enter Arrange mode (moving the quick buttons).
    var arrange: () -> Void
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @AppStorage(Pref.keyboard) private var showKeyboard = true
    @AppStorage(Pref.functionBar) private var showFunctionBar = true
    @AppStorage(Pref.dpad) private var showDpad = true
    @AppStorage(Pref.f12Button) private var showF12 = true
    @AppStorage(Pref.backspaceButton) private var showBackspace = true
    @AppStorage(Pref.banner) private var showBanner = true
    @AppStorage(Pref.stats) private var showStats = false
    @AppStorage(Pref.buttonOpacity) private var buttonOpacity = 0.6

    var body: some View {
        NavigationStack {
            Form {
                Section("Show") {
                    Toggle("Keyboard", isOn: $showKeyboard)
                    Toggle("Function keys", isOn: $showFunctionBar)
                    Toggle("Control banner", isOn: $showBanner)
                    Toggle("Stats", isOn: $showStats)
                    Toggle("Sound", isOn: Binding(get: { !model.muted }, set: { model.muted = !$0 }))
                }
                Section {
                    Toggle("Predictive cursor", isOn: Binding(get: { model.predictiveCursor }, set: { model.predictiveCursor = $0 }))
                } footer: {
                    Text(model.state.stream?.internet == true
                         ? "Draws a pointer ahead of the MiSTer's own to hide the delay. Over the internet the delay jumps around, so it's off unless you turn it on; home and the internet each remember their own setting."
                         : "Draws a pointer ahead of the MiSTer's own to hide the delay. Home and the internet each remember their own setting.")
                }
                Section("Quick buttons") {
                    Toggle("Arrows and OK", isOn: $showDpad)
                    Toggle("F12 (MiSTer menu)", isOn: $showF12)
                    Toggle("Backspace", isOn: $showBackspace)
                    if showDpad || showF12 || showBackspace {
                        HStack {
                            Text("See-through")
                            Slider(value: $buttonOpacity, in: 0.25...1)
                        }
                        Button("Move buttons…") {
                            dismiss()
                            arrange()
                        }
                        Button("Reset positions") { QuickButtons.resetPositions() }
                    }
                }
                Section {
                    NavigationLink("Type text…") { TypeTextScreen(done: { dismiss() }) }
                        .disabled(!model.isConnected)
                    Button("Reset zoom") { model.resetZoom() }
                    if model.someoneElseInControl {
                        Button("Take control") { model.takeControl(); dismiss() }
                    }
                } footer: {
                    Text("Three-finger tap: this menu, even with everything hidden. Three-finger swipe: next or previous MiSTer.")
                }
                Section {
                    Button("Disconnect", role: .destructive) {
                        dismiss()
                        model.close()
                    }
                }
            }
            .navigationTitle(model.profile?.name ?? "MiSTer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }
}

/// Every saved MiSTer, the open one ticked: tap another to switch to it, or
/// disconnect and go back to the list.
struct MiSTerSwitcher: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(model.profiles) { profile in
                        Button {
                            dismiss()
                            model.switchTo(profile)
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(profile.name)
                                    Text(profile.subtitle).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                if profile.id == model.state.profileId {
                                    Image(systemName: "checkmark").foregroundStyle(Color.accentColor)
                                }
                            }
                        }
                        .tint(.primary)
                    }
                } footer: {
                    Text("Three-finger swipe left or right also switches.")
                }
                Section {
                    Button("Disconnect", role: .destructive) {
                        dismiss()
                        model.close()
                    }
                }
            }
            .navigationTitle("MiSTers")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }
}
