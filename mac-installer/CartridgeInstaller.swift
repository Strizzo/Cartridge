import AppKit
import Combine
import Foundation
import SwiftUI

private enum Palette {
    static let background = Color(red: 0.09, green: 0.09, blue: 0.09)
    static let panel = Color(red: 0.13, green: 0.13, blue: 0.13)
    static let border = Color(red: 0.29, green: 0.28, blue: 0.27)
    static let paper = Color(red: 0.96, green: 0.95, blue: 0.91)
    static let muted = Color(red: 0.59, green: 0.57, blue: 0.54)
    static let red = Color(red: 0.94, green: 0.27, blue: 0.21)
}

private struct Card: Decodable, Identifiable {
    let identifier: String
    let media_name: String
    let size_bytes: Int64
    let status: String
    let detail: String
    let inventory_fingerprint: String
    let partitions: [CardPartition]
    var id: String { identifier }
    var sizeLabel: String { String(format: "%.1f GB", Double(size_bytes) / 1_000_000_000) }
    var partitionLabel: String {
        partitions.map { $0.volume_name.isEmpty ? $0.content : $0.volume_name }.joined(separator: " · ")
    }
    var isPotentialSpare: Bool {
        status == "unsupported_layout" && partitions.count == 1 && partitions[0].filesystem == "exfat"
    }
}

private struct CardPartition: Decodable {
    let content: String
    let filesystem: String
    let volume_name: String
}

private struct CardsResponse: Decodable { let cards: [Card] }

private struct ImageInfo: Decodable {
    let image: String
    let volume: String
    let card_bytes: Int64
    let build_revision: String
    let boot_logo_sha256: String?
}

private struct BackendState: Decodable { let state: String }

@MainActor
private final class InstallerModel: ObservableObject {
    @Published var cards: [Card] = []
    @Published var selectedDisk: String?
    @Published var imageURL: URL?
    @Published var imageInfo: ImageInfo?
    @Published var status = "Insert a spare microSD card to begin."
    @Published var detail = "The installer will never select a disk automatically."
    @Published var progress = ""
    @Published var busy = false
    @Published var preflightPassed = false
    @Published var eraseConfirmed = false

    private let bridge: String
    private let guardPath: String

    init() {
        let resources = Bundle.main.resourceURL!
        bridge = resources.appendingPathComponent("installer/mac_bridge.py").path
        guardPath = resources.appendingPathComponent("disk-mount-guard").path
        refreshCards()
    }

    var selectedCard: Card? { cards.first { $0.identifier == selectedDisk } }
    var canPreflight: Bool {
        !busy && selectedCard?.isPotentialSpare == true && imageInfo != nil &&
        selectedCard?.size_bytes == imageInfo?.card_bytes
    }
    var canInstall: Bool { canPreflight && preflightPassed && eraseConfirmed }

    nonisolated private static func run(_ arguments: [String], bridge: String) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = [bridge] + arguments
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        let output = stdout.fileHandleForReading.readDataToEndOfFile()
        let error = stderr.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            throw NSError(domain: "CartridgeInstaller", code: Int(process.terminationStatus),
                          userInfo: [NSLocalizedDescriptionKey:
                            String(data: error, encoding: .utf8) ?? "Installer check failed."])
        }
        return String(data: output, encoding: .utf8) ?? ""
    }

    nonisolated private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    nonisolated private static func privileged(_ arguments: [String], bridge: String) throws -> String {
        let command = (["/usr/bin/caffeinate", "-i", "-m", "/usr/bin/python3", bridge] + arguments)
            .map(shellQuote).joined(separator: " ")
        let literal = command.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let script = NSAppleScript(source: "do shell script \"\(literal)\" with administrator privileges")
        var error: NSDictionary?
        let result = script?.executeAndReturnError(&error)
        if let error {
            throw NSError(domain: "CartridgeInstaller", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: error.description])
        }
        return result?.stringValue ?? ""
    }

    private func perform(_ label: String, arguments: [String], administrator: Bool = false,
                         done: @escaping (Result<String, Error>) -> Void) {
        busy = true
        status = label
        detail = administrator ? "macOS will ask for administrator access to the selected card." : ""
        let script = bridge
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result {
                try administrator ? Self.privileged(arguments, bridge: script) :
                    Self.run(arguments, bridge: script)
            }
            DispatchQueue.main.async {
                self.busy = false
                done(result)
            }
        }
    }

    func refreshCards() {
        perform("Checking connected cards…", arguments: ["cards"]) { result in
            switch result {
            case .success(let text):
                do {
                    self.cards = try JSONDecoder().decode(CardsResponse.self, from: Data(text.utf8)).cards
                    if !self.cards.contains(where: { $0.identifier == self.selectedDisk }) {
                        self.selectedDisk = nil
                        self.preflightPassed = false
                    }
                    self.status = "Select the card you want to prepare."
                    self.detail = "Existing game cards are shown for identification but cannot be erased in this trial installer."
                } catch { self.fail(error) }
            case .failure(let error): self.fail(error)
            }
        }
    }

    func select(_ card: Card) {
        selectedDisk = card.identifier
        preflightPassed = false
        eraseConfirmed = false
        status = card.isPotentialSpare ? "Spare card selected." : "Existing or unsupported card selected."
        detail = card.isPotentialSpare ?
            "A read-only preflight will still verify that this exFAT card is empty." :
            "This installer will not write to an existing game card."
    }

    func chooseImage() {
        let picker = NSOpenPanel()
        picker.canChooseFiles = true
        picker.canChooseDirectories = true
        picker.allowsMultipleSelection = false
        picker.message = "Choose a verified CartridgeOS first-boot .sparsebundle"
        guard picker.runModal() == .OK, let url = picker.url else { return }
        imageURL = url
        imageInfo = nil
        preflightPassed = false
        eraseConfirmed = false
        perform("Checking image report…", arguments: ["image", "--image", url.path]) { result in
            switch result {
            case .success(let text):
                do {
                    self.imageInfo = try JSONDecoder().decode(ImageInfo.self, from: Data(text.utf8))
                    self.status = "Image report selected."
                    self.detail = "Build \(self.imageInfo!.build_revision.prefix(12)) · Run preflight to verify image contents."
                } catch { self.fail(error) }
            case .failure(let error): self.fail(error)
            }
        }
    }

    private func selectedArguments(_ action: String) -> [String]? {
        guard let card = selectedCard, let image = imageURL else { return nil }
        return [action, "--disk", card.identifier, "--fingerprint", card.inventory_fingerprint,
                "--image", image.path]
    }

    func preflight() {
        guard canPreflight, let arguments = selectedArguments("preflight") else { return }
        perform("Checking the spare card and image read-only…", arguments: arguments) { result in
            switch result {
            case .success(let text):
                do {
                    let state = try JSONDecoder().decode(BackendState.self, from: Data(text.utf8)).state
                    self.preflightPassed = state == "preflight_passed"
                    self.status = self.preflightPassed ? "Preflight passed." : "Preflight did not pass."
                    self.detail = self.preflightPassed ?
                        "The card is empty, the image matches its report, and the target is removable." : text
                } catch { self.fail(error) }
            case .failure(let error): self.fail(error)
            }
        }
    }

    func install() {
        guard canInstall, var arguments = selectedArguments("write") else { return }
        arguments += ["--mount-guard", guardPath]
        perform("Writing the selected spare card…", arguments: arguments, administrator: true) { result in
            switch result {
            case .success(let text):
                self.status = text.contains("verified_and_ejected") ?
                    "Card verified and ejected." : "Write finished; readback is still required."
                self.detail = text
            case .failure(let error):
                self.fail(error)
                self.detail += " If the card was ejected after writing, reinsert it and use Verify readback."
            }
        }
    }

    func verifyReadback() {
        guard !busy, let arguments = selectedArguments("verify") else { return }
        perform("Reading back the entire card…", arguments: arguments, administrator: true) { result in
            switch result {
            case .success(let text):
                self.status = "Card verified and ejected."
                self.detail = text
            case .failure(let error):
                self.fail(error)
                self.detail += " Keep the card out of the handheld until the mismatch is reviewed."
            }
        }
    }

    func pollProgress() {
        guard busy, let imageURL else { return }
        let report = imageURL.deletingLastPathComponent().appendingPathComponent("spare-write-report.json")
        guard let data = try? Data(contentsOf: report),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let state = json["state"] as? String else { return }
        if let written = json["bytes_written"] as? Int64,
           let total = json["card_bytes"] as? Int64, total > 0 {
            progress = String(format: "Writing %.1f / %.1f GB", Double(written)/1e9, Double(total)/1e9)
        } else { progress = state.replacingOccurrences(of: "_", with: " ").uppercased() }
    }

    private func fail(_ error: Error) {
        status = "Stopped safely."
        detail = error.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

private struct InstallerView: View {
    @StateObject private var model = InstallerModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text("CARTRIDGE").font(.system(size: 36, weight: .black, design: .monospaced))
                Text("OS / INSTALLER").font(.system(size: 12, weight: .bold, design: .monospaced))
                    .foregroundStyle(Palette.muted)
                Spacer()
                Text("MACOS  ·  LOCAL IMAGE").font(.system(size: 11, weight: .bold, design: .monospaced))
                    .foregroundStyle(Palette.red)
            }
            .padding(.horizontal, 30).padding(.top, 25).padding(.bottom, 18)
            Rectangle().fill(Palette.red).frame(height: 6)

            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    section("01", "SELECT A CARD") {
                        HStack {
                            Text("Only a selected, empty removable card can be written.")
                                .font(.system(size: 13)).foregroundStyle(Palette.muted)
                            Spacer()
                            Button("Refresh") { model.refreshCards() }.disabled(model.busy)
                        }
                        if model.cards.isEmpty {
                            Text("No external cards detected. Insert one, then refresh.")
                                .foregroundStyle(Palette.paper).padding(.vertical, 18)
                        }
                        ForEach(model.cards) { card in
                            Button { model.select(card) } label: {
                                HStack(spacing: 14) {
                                    Image(systemName: model.selectedDisk == card.id ? "largecircle.fill.circle" : "circle")
                                        .foregroundStyle(model.selectedDisk == card.id ? Palette.red : Palette.muted)
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text("\(card.identifier.uppercased())  ·  \(card.media_name)")
                                            .font(.system(size: 17, weight: .bold, design: .monospaced))
                                        Text(card.partitionLabel.isEmpty ? "No visible partitions" : card.partitionLabel)
                                            .font(.system(size: 11, design: .monospaced))
                                            .foregroundStyle(Palette.muted)
                                    }
                                    Spacer()
                                    VStack(alignment: .trailing, spacing: 4) {
                                        Text(card.sizeLabel).font(.system(size: 16, weight: .bold, design: .monospaced))
                                        Text(card.isPotentialSpare ? "EMPTY-CARD CHECK AVAILABLE" : "PRESERVE / READ ONLY")
                                            .font(.system(size: 10, weight: .bold, design: .monospaced))
                                            .foregroundStyle(card.isPotentialSpare ? Palette.red : Palette.muted)
                                    }
                                }
                                .foregroundStyle(Palette.paper)
                                .padding(16)
                                .background(Palette.panel)
                                .overlay(Rectangle().stroke(model.selectedDisk == card.id ? Palette.red : Palette.border,
                                                            lineWidth: model.selectedDisk == card.id ? 2 : 1))
                            }.buttonStyle(.plain)
                        }
                    }

                    section("02", "CHOOSE A VERIFIED IMAGE") {
                        HStack {
                            VStack(alignment: .leading, spacing: 5) {
                                Text(model.imageURL?.lastPathComponent ?? "No image selected")
                                    .foregroundStyle(Palette.paper).font(.system(size: 15, weight: .semibold))
                                Text(model.imageInfo.map { "BUILD \($0.build_revision.prefix(12))  ·  \(String(format: "%.1f", Double($0.card_bytes)/1e9)) GB" }
                                     ?? "A local first-boot .sparsebundle and matching build report are required.")
                                    .foregroundStyle(Palette.muted).font(.system(size: 11, design: .monospaced))
                            }
                            Spacer()
                            Button("Choose image…") { model.chooseImage() }.disabled(model.busy)
                        }
                        .padding(18).background(Palette.panel)
                        .overlay(Rectangle().stroke(Palette.border))
                    }

                    section("03", "CHECK AND INSTALL") {
                        HStack(spacing: 14) {
                            Button("Run read-only preflight") { model.preflight() }
                                .disabled(!model.canPreflight)
                            Text(model.preflightPassed ? "PREFLIGHT PASSED" : "NOT YET CHECKED")
                                .font(.system(size: 11, weight: .bold, design: .monospaced))
                                .foregroundStyle(model.preflightPassed ? Palette.red : Palette.muted)
                            Spacer()
                        }
                        Toggle("I understand this selected empty card will be completely replaced.",
                               isOn: $model.eraseConfirmed)
                            .toggleStyle(.checkbox).disabled(!model.preflightPassed || model.busy)
                            .foregroundStyle(Palette.paper)
                        HStack {
                            Button("WRITE SELECTED CARD") { model.install() }
                                .buttonStyle(PrimaryButtonStyle()).disabled(!model.canInstall)
                            Button("Verify reinserted card") { model.verifyReadback() }
                                .disabled(model.busy || model.imageInfo == nil || model.selectedDisk == nil)
                            Spacer()
                            if model.busy { ProgressView().controlSize(.small) }
                        }
                    }

                    HStack(spacing: 12) {
                        Rectangle().fill(Palette.red).frame(width: 4)
                        VStack(alignment: .leading, spacing: 6) {
                            Text(model.status.uppercased())
                                .font(.system(size: 15, weight: .heavy, design: .monospaced))
                                .foregroundStyle(Palette.paper)
                            Text(model.progress.isEmpty ? model.detail : model.progress + "  ·  " + model.detail)
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(Palette.muted).textSelection(.enabled)
                        }
                        Spacer()
                    }
                    .padding(18).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.panel)
                }
                .padding(30)
            }
        }
        .frame(minWidth: 900, minHeight: 760)
        .background(Palette.background)
        .preferredColorScheme(.dark)
        .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { _ in
            model.pollProgress()
        }
    }

    private func section<Content: View>(_ number: String, _ title: String,
                                        @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Text(number).foregroundStyle(Palette.red)
                Text(title).foregroundStyle(Palette.paper)
            }
            .font(.system(size: 17, weight: .heavy, design: .monospaced))
            content()
        }
    }
}

private struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .heavy, design: .monospaced))
            .foregroundStyle(Palette.background)
            .padding(.horizontal, 20).padding(.vertical, 12)
            .background(Palette.red.opacity(configuration.isPressed ? 0.75 : 1))
    }
}

@main
struct CartridgeInstallerApp: App {
    var body: some Scene {
        WindowGroup("CartridgeOS Installer") { InstallerView() }
    }
}
