#if canImport(KVMSCore)
import KVMSCore
#endif
import SwiftUI

struct ProfileEditor: View {
    @Environment(\.dismiss) private var dismiss
    @State var profile: HostProfile
    let onSave: (HostProfile) -> Void
    private let original: HostProfile
    /// "capture" or "local".
    @State private var source: String
    // Capture server streams
    @StateObject private var finder = StreamFinder()
    /// The stream being resolved to an address after it was picked.
    @State private var resolvingStream: StreamFinder.Found.ID?
    @State private var streamError: String?
    // Capture cards on this Mac
    @State private var cards: [CaptureCard] = []
    @State private var audioInputs: [AudioInput] = []
    @State private var searchingCards = false
    // A JetKVM as the "card": its password (Keychain) and address to add by hand.
    @State private var jetkvmPassword = ""
    @State private var jetkvmAddress = ""

    init(profile: HostProfile, onSave: @escaping (HostProfile) -> Void) {
        _profile = State(initialValue: profile)
        self.onSave = onSave
        original = profile
        let isNew = profile.misterHost.isEmpty && profile.stream.isEmpty && profile.localVideo.isEmpty
        // New hosts start on the capture server; old ones keep what they use.
        _source = State(initialValue: isNew ? "capture" : profile.sourceKind)
    }

    /// The stream, once a search finds just one and none was set.
    private func pickOnlyStream() {
        if profile.stream.isEmpty, resolvingStream == nil, finder.found.count == 1 {
            chooseStream(finder.found[0])
        }
    }

    /// Resolves the picked stream to the address this Mac reaches it on.
    private func chooseStream(_ found: StreamFinder.Found) {
        resolvingStream = found.id
        streamError = nil
        Task {
            let address = await StreamFinder.address(of: found)
            guard resolvingStream == found.id else { return }
            resolvingStream = nil
            guard let address else {
                streamError = "\(found.name) didn't answer. Is this Mac on the same network as \(found.server.isEmpty ? "the capture PC" : found.server)?"
                return
            }
            profile.stream = address
            // The server's name, unless one was typed.
            if profile.nameFromServer || profile.name.isEmpty {
                profile.name = found.name
            }
        }
    }

    /// Capture cards and audio inputs on this Mac.
    private func findCards() {
        searchingCards = true
        Task.detached {
            let found = KVMSCore.captureCards()
            let audio = KVMSCore.audioInputs()
            await MainActor.run {
                searchingCards = false
                cards = found
                audioInputs = audio
                if profile.localVideo.isEmpty, let only = found.first, found.count == 1 {
                    chooseCard(only)
                }
            }
        }
    }

    private func chooseCard(_ card: CaptureCard) {
        profile.localVideo = card.id
        if !card.audio.isEmpty { profile.localAudio = card.audio }
        loadJetKVMPassword()
    }

    private var isJetKVM: Bool { profile.localVideo.hasPrefix("jetkvm://") }

    private func loadJetKVMPassword() {
        jetkvmPassword = isJetKVM ? JetKVMPasswords.load(profile.localVideo) : ""
    }

    /// A JetKVM the scan didn't find (another network): by its address.
    private func addJetKVM() {
        let key = JetKVMPasswords.key(jetkvmAddress)
        guard !key.isEmpty else { return }
        let id = "jetkvm://\(key)"
        if !cards.contains(where: { $0.id == id }) {
            cards.append(CaptureCard(id: id, name: "JetKVM (\(key))", audio: ""))
        }
        profile.localVideo = id
        jetkvmAddress = ""
        loadJetKVMPassword()
    }

    private func showSource() {
        switch source {
        case "local": if cards.isEmpty { findCards() }
        default: if !finder.searching && finder.found.isEmpty { finder.start() }
        }
    }

    var body: some View {
        Form {
            Section("Host") {
                TextField("Name", text: $profile.name,
                          prompt: Text(source == "local" ? "MiSTer" : "As the capture server calls it"))
            }
            Section {
                Picker("Video & audio from", selection: $source) {
                    Text("MiSTerKVMS Capture (a stream on the network)").tag("capture")
                    Text("A capture card on this Mac, or a JetKVM").tag("local")
                }
            } header: {
                Text("Video & audio")
            } footer: {
                if source == "capture" {
                    Text("Keyboard and mouse go through the capture server, which knows this MiSTer's address.")
                        .font(.caption)
                }
            }
            switch source {
            case "local": cardSection
            default: streamSection
            }
            if source == "local" && isJetKVM {
                jetkvmSection
            }
            if source == "local" {
                Section {
                    TextField("MiSTer IP or hostname", text: $profile.misterHost, prompt: Text("192.168.1.50"))
                    TextField("mrext port", value: $profile.misterPort, format: .number.grouping(.never))
                } header: {
                    Text(isJetKVM && profile.input != "mrext"
                         ? "MiSTer (optional: mrext Remote for the core's name)"
                         : "MiSTer (keyboard & mouse via mrext Remote)")
                }
            }
            Section("Mouse") {
                LabeledContent("Sensitivity") {
                    HStack {
                        Slider(value: $profile.mouseSensitivity, in: 0.1...4, step: 0.05)
                        Text(profile.mouseSensitivity, format: .number.precision(.fractionLength(2)))
                            .monospacedDigit().frame(width: 40)
                    }
                }
                Toggle("⌥-click sends middle click", isOn: $profile.altClickIsMiddle)
                Toggle(isOn: $profile.rawMouse) {
                    Text("Raw mouse input")
                    Text("Sends your mouse's own movement without macOS acceleration, so only the vintage OS accelerates it, like a mouse plugged straight in. Trackpads keep using macOS movement.")
                }
                if profile.rawMouse && !RawMouse.hasPermission {
                    LabeledContent {
                        Button("Allow…") { RawMouse.requestPermission() }
                    } label: {
                        Text("Needs Input Monitoring permission")
                        Text("macOS asks once; the setting is in Privacy & Security → Input Monitoring.")
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 500)
        .onAppear {
            showSource()
            loadJetKVMPassword()
        }
        .onChange(of: source) { _, _ in showSource() }
        .onChange(of: finder.searching) { _, searching in if !searching { pickOnlyStream() } }
        .onDisappear { finder.stop() }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    profile.misterHost = profile.misterHost.trimmingCharacters(in: .whitespaces)
                    profile.stream = profile.stream.trimmingCharacters(in: .whitespaces)
                    profile.source = source
                    if source == "local" && isJetKVM {
                        JetKVMPasswords.save(jetkvmPassword, for: profile.localVideo)
                    }
                    // Mouse scaling changed: the predictive cursor must relearn it.
                    if profile.rawMouse != original.rawMouse || profile.mouseSensitivity != original.mouseSensitivity {
                        profile.cursorGain = 0
                    }
                    onSave(profile)
                    dismiss()
                }
            }
        }
    }
}

extension ProfileEditor {
    @ViewBuilder fileprivate var streamSection: some View {
        Section {
            LabeledContent("Stream") {
                HStack(spacing: 6) {
                    TextField("Stream", text: $profile.stream, prompt: Text("Choose, or type mkvm://pc/stream"))
                        .labelsHidden()
                    Menu {
                        if finder.found.isEmpty {
                            Text(finder.searching ? "Looking for streams…" : "No streams found")
                        }
                        ForEach(finder.found) { found in
                            Button(found.mister.isEmpty ? found.name : "\(found.name) (\(found.mister))") {
                                chooseStream(found)
                            }
                        }
                    } label: {
                        Image(systemName: "chevron.up.chevron.down")
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                    .help("Streams found on the network")
                    Button(action: { streamError = nil; finder.start() }) {
                        if finder.searching || resolvingStream != nil {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("Find")
                        }
                    }
                    .disabled(finder.searching || resolvingStream != nil)
                }
            }
            if let streamError {
                Text(streamError).font(.caption).foregroundStyle(.red)
            } else if let resolvingStream, let found = finder.found.first(where: { $0.id == resolvingStream }) {
                Text("Connecting to \(found.name)…").font(.caption).foregroundStyle(.secondary)
            } else if !finder.searching {
                Text(finder.found.isEmpty
                     ? "No streams found. Is MiSTerKVMS Capture running on the capture PC, with a stream set up? You can also type its address."
                     : finder.found.count == 1 ? "Found 1 stream." : "Found \(finder.found.count) streams.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Picker("Quality", selection: $profile.quality) {
                Text("Automatic").tag("auto")
                Text("Best (full size, for wired networks)").tag("best")
                Text("Balanced (up to 1080p)").tag("balanced")
                Text("Low (up to 720p, for slow Wi-Fi)").tag("low")
            }
        } footer: {
            Text("Streams come from MiSTerKVMS Capture running on the PC with the capture card. Several people can watch; one at a time controls the MiSTer.")
                .font(.caption)
        }
        .onAppear { if profile.quality.isEmpty { profile.quality = "auto" } }
    }

    @ViewBuilder fileprivate var cardSection: some View {
        Section {
            LabeledContent(isJetKVM ? "JetKVM" : "Capture card") {
                HStack(spacing: 6) {
                    Picker("Capture card", selection: Binding(
                        get: { profile.localVideo },
                        set: { id in
                            if let card = cards.first(where: { $0.id == id }) { chooseCard(card) } else { profile.localVideo = id }
                        }
                    )) {
                        if profile.localVideo.isEmpty { Text("Choose a card").tag("") }
                        ForEach(cards, id: \.id) { Text($0.name).tag($0.id) }
                        if !profile.localVideo.isEmpty, !cards.contains(where: { $0.id == profile.localVideo }) {
                            Text(searchingCards ? "…" : "Not connected").tag(profile.localVideo)
                        }
                    }
                    .labelsHidden()
                    Button(action: findCards) {
                        if searchingCards {
                            ProgressView().controlSize(.small)
                        } else {
                            Text("Refresh")
                        }
                    }
                    .disabled(searchingCards)
                }
            }
            Picker("Audio", selection: $profile.localAudio) {
                Text("No audio").tag("")
                ForEach(audioInputs, id: \.id) { Text($0.name).tag($0.id) }
                if !profile.localAudio.isEmpty, !audioInputs.contains(where: { $0.id == profile.localAudio }) {
                    Text(profile.localAudio).tag(profile.localAudio)
                }
            }
            if !searchingCards, cards.isEmpty {
                Text("No capture cards or JetKVMs found.").font(.caption).foregroundStyle(.secondary)
            }
            LabeledContent("JetKVM not listed?") {
                HStack(spacing: 6) {
                    TextField("JetKVM address", text: $jetkvmAddress, prompt: Text("192.168.1.60"))
                        .labelsHidden()
                        .onSubmit(addJetKVM)
                    Button("Add", action: addJetKVM).disabled(jetkvmAddress.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        } footer: {
            Text("Only one program can use a capture card at a time: close OBS or MiSTerKVMS Capture if they have it open. macOS asks once for Camera and Microphone access.")
                .font(.caption)
        }
    }

    @ViewBuilder fileprivate var jetkvmSection: some View {
        Section {
            SecureField("Password", text: $jetkvmPassword, prompt: Text("None"))
            Picker("Keyboard & mouse", selection: Binding(
                get: { profile.input == "mrext" ? "mrext" : "auto" },
                set: { profile.input = $0 }
            )) {
                Text("Through the JetKVM's USB").tag("auto")
                Text("Through mrext Remote").tag("mrext")
            }
        } header: {
            Text("JetKVM")
        } footer: {
            Text("The password is kept in your Keychain. The JetKVM's USB is a real USB keyboard and mouse to the MiSTer. A JetKVM Mini only takes 1080p at 30 Hz (set it in MiSTer.ini) and has no HDMI sound: choose the input the MiSTer's sound goes to under Audio. One viewer at a time: opening the JetKVM's web page disconnects this Mac.")
                .font(.caption)
        }
    }
}
