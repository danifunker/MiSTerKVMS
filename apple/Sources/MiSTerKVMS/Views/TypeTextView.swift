import AppKit
#if canImport(KVMSCore)
import KVMSCore
#endif
import SwiftUI

/// Session ▸ Type Text…: text (the clipboard, to start with) typed on the
/// MiSTer key by key, as on a US keyboard. The speed is for the core: some
/// take keys slowly (computer cores); it's remembered for the host.
struct TypeTextView: View {
    @EnvironmentObject private var controller: SessionController
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var speed = 10.0

    private static let range = typeSpeedRange()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Type Text on the MiSTer").font(.headline)
            Text("Typed key by key, as on a US keyboard. The MiSTer has no clipboard.")
                .font(.callout).foregroundStyle(.secondary)
            TextEditor(text: $text)
                .font(.body.monospaced())
                .frame(minHeight: 160)
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(.separator))
            HStack {
                Text("Speed")
                Slider(value: $speed, in: Self.range[0]...Self.range[1]) {
                    Text("Speed")
                } minimumValueLabel: {
                    Text("Slow").font(.caption)
                } maximumValueLabel: {
                    Text("Fast").font(.caption)
                }
                .labelsHidden()
                Text("\(Int(speed.rounded())) keys/s").monospacedDigit().frame(width: 76, alignment: .trailing)
            }
            Text(summary).font(.caption).foregroundStyle(controller.canType ? Color.secondary : Color.orange)
            HStack {
                Text("Missed letters? Slow it down for this MiSTer.").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Type") {
                    controller.typeText(text, keysPerSec: speed.rounded())
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(preview.keys == 0 || !controller.canType)
            }
        }
        .padding(20)
        .frame(width: 540)
        .onAppear {
            text = NSPasteboard.general.string(forType: .string) ?? ""
            let saved = controller.profile?.typeSpeed ?? 0
            speed = saved > 0 ? saved : Self.range[2]
        }
    }

    private var preview: TextTyping { textTyping(text: text, keysPerSec: speed.rounded()) }

    private var summary: String {
        if !controller.canType { return "Someone else has control of this MiSTer: take control first." }
        let p = preview
        if p.keys == 0 { return "Nothing to type." }
        let time = p.seconds < 60 ? "\(Int(p.seconds.rounded(.up))) s" : "\(Int((p.seconds / 60).rounded(.up))) min"
        var line = "\(p.keys) keys, about \(time)"
        if p.skipped > 0 {
            line += " · \(p.skipped) character\(p.skipped == 1 ? "" : "s") can't be typed on a US keyboard and will be skipped"
        }
        return line
    }
}
