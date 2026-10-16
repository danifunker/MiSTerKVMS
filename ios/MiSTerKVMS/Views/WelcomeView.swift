import SwiftUI

/// The first-run screen (docs/APP-STORE.md S1): what the app needs, and a
/// way to try it without any of that. Shown once, and only to a phone with
/// no MiSTers saved.
struct WelcomeView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            Image(systemName: "display")
                .font(.system(size: 56, weight: .light))
                .foregroundStyle(.tint)
                .padding(.bottom, 16)
            Text("MiSTerKVMS")
                .font(.largeTitle.weight(.bold))
            Text("Your MiSTer's screen and sound on your iPhone, with its keyboard and mouse under your fingers.")
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.top, 8)
                .padding(.horizontal, 8)

            VStack(alignment: .leading, spacing: 14) {
                need("cpu", "A MiSTer FPGA running mrext Remote")
                need("cable.connector.horizontal", "A capture card showing its HDMI output")
                need("desktopcomputer", "MiSTerKVMS Capture on the computer the card is plugged into (Windows, Mac or Linux)")
            }
            .padding(.top, 32)
            .padding(.horizontal, 8)

            Spacer()

            VStack(spacing: 12) {
                Button {
                    model.welcomed(demo: false)
                } label: {
                    Text("I have MiSTerKVMS Capture set up")
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)

                Button {
                    model.welcomed(demo: true)
                } label: {
                    Text("Try it without a MiSTer")
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.bordered)

                Text("Shows a test pattern and lists what you type and tap. Turn it off in Settings.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 16)
    }

    private func need(_ symbol: String, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: symbol)
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 30)
            Text(text).font(.callout)
        }
    }
}

/// A demo host's input over the picture: what a MiSTer would have
/// received, newest last, each line gone after 5 seconds (the core keeps
/// the list).
struct DemoInputList: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.15)) { _ in
            let lines = model.core.demoInput()
            VStack(alignment: .leading, spacing: 2) {
                Text("Demo: what the MiSTer would receive")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                if lines.isEmpty {
                    Text("Tap, drag or type…")
                        .font(.footnote.monospaced())
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                        Text(line).font(.footnote.monospaced())
                    }
                }
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.6)))
            .allowsHitTesting(false)
            .accessibilityElement(children: .combine)
        }
    }
}
