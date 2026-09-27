import SwiftUI
import AltimKit

/// Connection to the private Altim server (the Heroku app): address, then user + password
/// (+ 6-digit code only if 2FA is on the server).
struct LoginView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("serverInput") private var server = ""
    @AppStorage("lastUser") private var user = ""
    @State private var password = ""
    @State private var code = ""
    @State private var mode: AccessMode?
    @State private var url: URL?
    @State private var busy = false
    @State private var error: String?
    @FocusState private var field: Field?

    enum Field { case server, user, password, code }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("ALTIM")
                    .font(Theme.display(48))
                    .foregroundStyle(Theme.accentGradient)
                    .neonGlow(Theme.cyan, radius: 12)
                    .padding(.top, 32)
                Text("Accès privé").font(.title2.bold())
                Text("Altim se connecte à votre serveur (l'application Heroku) : toutes les analyses y sont calculées sur 40 sources. Le mot de passe est gardé chiffré dans le trousseau de cet iPhone.")
                    .font(.subheadline).foregroundStyle(Theme.textSecondary)

                VStack(alignment: .leading, spacing: 14) {
                    labeled("Adresse du serveur") {
                        TextField("mon-app.herokuapp.com", text: $server)
                            .textContentType(.URL)
                            .keyboardType(.URL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .focused($field, equals: .server)
                            .submitLabel(.next)
                            .onSubmit { Task { await check() } }
                            .onChange(of: server) { _, _ in mode = nil }
                    }
                    if case let .login(needsCode) = mode {
                        labeled("Identifiant") {
                            TextField("max", text: $user)
                                .textContentType(.username)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .focused($field, equals: .user)
                                .submitLabel(.next)
                                .onSubmit { field = .password }
                        }
                        labeled("Mot de passe") {
                            SecureField("••••••", text: $password)
                                .textContentType(.password)
                                .focused($field, equals: .password)
                                .submitLabel(needsCode ? .next : .go)
                                .onSubmit { if needsCode { field = .code } else { Task { await connect() } } }
                        }
                        if needsCode {
                            labeled("Code à 6 chiffres") {
                                TextField("123456", text: $code)
                                    .textContentType(.oneTimeCode)
                                    .keyboardType(.numberPad)
                                    .focused($field, equals: .code)
                            }
                            Text("Double authentification active sur le serveur : le code sera redemandé quand la session expire (7 jours).")
                                .font(.caption).foregroundStyle(Theme.textSecondary)
                        }
                    }
                    if mode == .open {
                        Notice(text: "Serveur de développement sans accès privé : aucune connexion demandée.", tone: .warn)
                    }
                }
                .glassCard()

                if let error { Notice(text: error, tone: .bad) }

                Button {
                    Task { mode == nil ? await check() : await connect() }
                } label: {
                    if busy { ProgressView().tint(.black) } else { Text(mode == nil ? "CONTINUER" : "SE CONNECTER") }
                }
                .buttonStyle(NeonButtonStyle())
                .disabled(busy || !canSubmit)
                .opacity(canSubmit ? 1 : 0.4)

                Text("Sans réseau vers le serveur, l'app ne peut rien afficher : aucune donnée de marché n'est inventée ni gardée en cache au-delà de quelques minutes.")
                    .font(.caption).foregroundStyle(Theme.textSecondary)
            }
            .padding(24)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(AppBackground())
        .onAppear { field = server.isEmpty ? .server : nil }
    }

    private var canSubmit: Bool {
        guard !server.trimmingCharacters(in: .whitespaces).isEmpty else { return false }
        if case let .login(needsCode) = mode {
            return !user.isEmpty && !password.isEmpty && (!needsCode || code.count == 6)
        }
        return true
    }

    private func labeled<C: View>(_ label: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.caption.weight(.semibold)).foregroundStyle(Theme.textSecondary)
            content()
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.06)))
        }
    }

    private func check() async {
        busy = true
        error = nil
        defer { busy = false }
        do {
            let (u, m) = try await model.probe(server)
            url = u
            mode = m
            if case .login = m { field = user.isEmpty ? .user : .password }
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func connect() async {
        guard let url, let mode else { return await check() }
        busy = true
        error = nil
        defer { busy = false }
        do {
            try await model.connect(url: url, mode: mode, user: user, password: password, code: code)
            password = ""
            code = ""
        } catch {
            self.error = error.localizedDescription
        }
    }
}
