import SwiftUI

/// Controls menu ▸ Type text…: text typed on the MiSTer key by key, as on a
/// US keyboard (the MiSTer has no clipboard). The speed is for the core:
/// some take keys slowly; it's remembered for the host.
struct TypeTextScreen: View {
    /// Closes the controls menu, so the typing can be watched.
    var done: () -> Void
    @EnvironmentObject private var model: AppModel
    @State private var text = ""
    @State private var speed = 10.0
    @FocusState private var editing: Bool

    private static let range = typeSpeedRange()

    var body: some View {
        Form {
            Section {
                TextEditor(text: $text)
                    .font(.body.monospaced())
                    .frame(minHeight: 140)
                    .focused($editing)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                // Apple's Paste button: no "Allow Paste?" prompt.
                PasteButton(payloadType: String.self) { strings in
                    if let first = strings.first { text = first }
                }
            } footer: {
                Text("Typed key by key, as on a US keyboard.")
            }
            Section {
                HStack {
                    Text("Slow").font(.caption)
                    Slider(value: $speed, in: Self.range[0]...Self.range[1], step: 1)
                    Text("Fast").font(.caption)
                }
                Text("\(Int(speed)) keys per second").font(.footnote).foregroundStyle(.secondary)
            } header: {
                Text("Speed")
            } footer: {
                Text("Missed letters? Slow it down for this MiSTer.")
            }
            Section {
                Button("Type") {
                    model.typeText(text, keysPerSec: speed)
                    done()
                }
                .disabled(preview.keys == 0 || !canType)
            } footer: {
                Text(summary)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle("Type text")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            let saved = model.profile?.typeSpeed ?? 0
            speed = saved > 0 ? saved.rounded() : Self.range[2]
        }
    }

    private var canType: Bool { model.isConnected && !model.someoneElseInControl && !model.watching }

    private var preview: TextTyping { textTyping(text: text, keysPerSec: speed) }

    private var summary: String {
        if !canType { return "Take control of this MiSTer first." }
        let p = preview
        if p.keys == 0 { return "Paste or type the text above." }
        let time = p.seconds < 60 ? "\(Int(p.seconds.rounded(.up))) s" : "\(Int((p.seconds / 60).rounded(.up))) min"
        var line = "\(p.keys) keys, about \(time)."
        if p.skipped > 0 {
            line += " \(p.skipped) character\(p.skipped == 1 ? "" : "s") can't be typed on a US keyboard and will be skipped."
        }
        return line
    }
}
