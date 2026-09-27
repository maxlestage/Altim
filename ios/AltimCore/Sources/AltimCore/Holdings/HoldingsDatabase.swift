import Foundation
#if canImport(SQLite3)
import SQLite3
#else
import CSQLite
#endif

/// A line of the portfolio actually held by the user.
public struct Holding: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var symbol: String
    public var kind: AssetClass
    public var name: String
    public var quantity: Double
    /// Average cost price (PRU), in USD.
    public var averagePrice: Double

    public init(id: String = UUID().uuidString, symbol: String, kind: AssetClass, name: String, quantity: Double, averagePrice: Double) {
        self.id = id
        self.symbol = symbol.uppercased()
        self.kind = kind
        self.name = name
        self.quantity = quantity
        self.averagePrice = averagePrice
    }

    public var isValid: Bool {
        !symbol.isEmpty && quantity.isFinite && quantity > 0 && averagePrice.isFinite && averagePrice >= 0
    }
}

public enum HoldingsDatabaseError: Error, LocalizedError, Equatable {
    case open(String)
    case sql(String)
    case invalidHolding
    case invalidFile

    public var errorDescription: String? {
        switch self {
        case let .open(m): return "Impossible d'ouvrir la base de données : \(m)"
        case let .sql(m): return "Erreur de base de données : \(m)"
        case .invalidHolding: return "Quantité ou prix d'achat invalide."
        case .invalidFile: return "Fichier d'import invalide."
        }
    }
}

/// SQLite storage of the holdings (and cash) on the device.
///
/// - Schema versioned by `PRAGMA user_version` (forward migrations).
/// - Integrity constraints on the SQLite side (quantity > 0, class, one line per asset).
/// - WAL journal: writes are atomic and do not block reads.
/// - JSON import/export in the same format as the Altim web app (data transfer).
public actor HoldingsDatabase {
    private var db: OpaquePointer?
    public nonisolated let path: String

    /// Database in Application Support (not synced with iCloud, not visible to the user).
    public static func defaultURL() throws -> URL {
        let dir = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return dir.appendingPathComponent("altim.sqlite")
    }

    /// `path`: file path, or ":memory:" for an in-memory database (tests).
    public init(path: String) throws {
        self.path = path
        var handle: OpaquePointer?
        let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(path, &handle, flags, nil) == SQLITE_OK, let handle else {
            let message = handle.map { String(cString: sqlite3_errmsg($0)) } ?? "inconnue"
            sqlite3_close(handle)
            throw HoldingsDatabaseError.open(message)
        }
        db = handle
        try Self.execute(handle, "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
        try Self.migrate(handle)
    }

    deinit {
        sqlite3_close(db)
    }

    // MARK: Schema

    static let migrations: [String] = [
        // v1
        """
        CREATE TABLE holdings (
            id TEXT PRIMARY KEY NOT NULL,
            symbol TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('crypto', 'stock')),
            name TEXT NOT NULL,
            quantity REAL NOT NULL CHECK (quantity > 0),
            average_price REAL NOT NULL CHECK (average_price >= 0),
            created_at REAL NOT NULL,
            updated_at REAL NOT NULL
        );
        CREATE UNIQUE INDEX holdings_asset ON holdings (kind, symbol);
        CREATE TABLE settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
        """,
    ]

    static func migrate(_ db: OpaquePointer) throws {
        let version = try scalarInt(db, "PRAGMA user_version")
        guard version < migrations.count else { return }
        try execute(db, "BEGIN IMMEDIATE")
        do {
            for (i, sql) in migrations.enumerated() where i >= version {
                try execute(db, sql)
            }
            try execute(db, "PRAGMA user_version = \(migrations.count)")
            try execute(db, "COMMIT")
        } catch {
            try? execute(db, "ROLLBACK")
            throw error
        }
    }

    public func schemaVersion() throws -> Int { try Self.scalarInt(handle(), "PRAGMA user_version") }

    // MARK: Holdings

    public func all() throws -> [Holding] {
        let stmt = try prepare("SELECT id, symbol, kind, name, quantity, average_price FROM holdings ORDER BY created_at, symbol")
        defer { sqlite3_finalize(stmt) }
        var out: [Holding] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            guard let kind = AssetClass(rawValue: text(stmt, 2)) else { continue }
            out.append(Holding(id: text(stmt, 0), symbol: text(stmt, 1), kind: kind, name: text(stmt, 3),
                               quantity: sqlite3_column_double(stmt, 4), averagePrice: sqlite3_column_double(stmt, 5)))
        }
        return out
    }

    /// Adds or updates a line. For a new purchase of an asset already held (`merge`),
    /// the quantity is added and the average cost price recomputed.
    @discardableResult
    public func upsert(_ holding: Holding, merge: Bool = false) throws -> Holding {
        guard holding.isValid else { throw HoldingsDatabaseError.invalidHolding }
        let existing = try all().first { $0.id == holding.id || ($0.kind == holding.kind && $0.symbol == holding.symbol) }
        var saved = holding
        if let existing {
            saved.id = existing.id
            if merge && existing.id != holding.id {
                let qty = existing.quantity + holding.quantity
                saved.averagePrice = (existing.quantity * existing.averagePrice + holding.quantity * holding.averagePrice) / qty
                saved.quantity = qty
            }
            let stmt = try prepare("UPDATE holdings SET symbol = ?, kind = ?, name = ?, quantity = ?, average_price = ?, updated_at = ? WHERE id = ?")
            defer { sqlite3_finalize(stmt) }
            bind(stmt, saved.symbol, 1); bind(stmt, saved.kind.rawValue, 2); bind(stmt, saved.name, 3)
            sqlite3_bind_double(stmt, 4, saved.quantity); sqlite3_bind_double(stmt, 5, saved.averagePrice)
            sqlite3_bind_double(stmt, 6, Date().timeIntervalSince1970); bind(stmt, saved.id, 7)
            try step(stmt)
        } else {
            let stmt = try prepare("INSERT INTO holdings (id, symbol, kind, name, quantity, average_price, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            defer { sqlite3_finalize(stmt) }
            let now = Date().timeIntervalSince1970
            bind(stmt, saved.id, 1); bind(stmt, saved.symbol, 2); bind(stmt, saved.kind.rawValue, 3); bind(stmt, saved.name, 4)
            sqlite3_bind_double(stmt, 5, saved.quantity); sqlite3_bind_double(stmt, 6, saved.averagePrice)
            sqlite3_bind_double(stmt, 7, now); sqlite3_bind_double(stmt, 8, now)
            try step(stmt)
        }
        return saved
    }

    public func delete(id: String) throws {
        let stmt = try prepare("DELETE FROM holdings WHERE id = ?")
        defer { sqlite3_finalize(stmt) }
        bind(stmt, id, 1)
        try step(stmt)
    }

    // MARK: Cash

    public func cash() throws -> Double {
        let stmt = try prepare("SELECT value FROM settings WHERE key = 'cash'")
        defer { sqlite3_finalize(stmt) }
        guard sqlite3_step(stmt) == SQLITE_ROW else { return 0 }
        return Double(text(stmt, 0)) ?? 0
    }

    public func setCash(_ value: Double) throws {
        guard value.isFinite, value >= 0 else { throw HoldingsDatabaseError.invalidHolding }
        let stmt = try prepare("INSERT INTO settings (key, value) VALUES ('cash', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        defer { sqlite3_finalize(stmt) }
        bind(stmt, String(value), 1)
        try step(stmt)
    }

    // MARK: JSON import / export (Altim web format)

    struct Transfer: Codable {
        var app: String?
        var version: Int?
        var cash: Double?
        var holdings: [Holding]
    }

    public func exportJSON() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(Transfer(app: "altim", version: 1, cash: try cash(), holdings: try all()))
    }

    /// Replaces the holdings with those from the file, in a single transaction (all or nothing).
    public func importJSON(_ data: Data) throws {
        guard let transfer = try? JSONDecoder().decode(Transfer.self, from: data) else { throw HoldingsDatabaseError.invalidFile }
        let valid = transfer.holdings.filter(\.isValid)
        let db = try handle()
        try Self.execute(db, "BEGIN IMMEDIATE")
        do {
            try Self.execute(db, "DELETE FROM holdings")
            for h in valid { try upsert(h, merge: true) }
            try setCash(max(0, transfer.cash ?? 0))
            try Self.execute(db, "COMMIT")
        } catch {
            try? Self.execute(db, "ROLLBACK")
            throw error
        }
    }

    // MARK: SQLite helpers

    private func handle() throws -> OpaquePointer {
        guard let db else { throw HoldingsDatabaseError.sql("base fermée") }
        return db
    }

    private func prepare(_ sql: String) throws -> OpaquePointer? {
        let db = try handle()
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
            throw HoldingsDatabaseError.sql(String(cString: sqlite3_errmsg(db)))
        }
        return stmt
    }

    private func step(_ stmt: OpaquePointer?) throws {
        let rc = sqlite3_step(stmt)
        guard rc == SQLITE_DONE || rc == SQLITE_ROW else {
            throw HoldingsDatabaseError.sql(String(cString: sqlite3_errmsg(try handle())))
        }
    }

    private func bind(_ stmt: OpaquePointer?, _ value: String, _ index: Int32) {
        // SQLITE_TRANSIENT: SQLite copies the string.
        sqlite3_bind_text(stmt, index, value, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
    }

    private func text(_ stmt: OpaquePointer?, _ column: Int32) -> String {
        sqlite3_column_text(stmt, column).map { String(cString: $0) } ?? ""
    }

    static func execute(_ db: OpaquePointer, _ sql: String) throws {
        var error: UnsafeMutablePointer<CChar>?
        guard sqlite3_exec(db, sql, nil, nil, &error) == SQLITE_OK else {
            let message = error.map { String(cString: $0) } ?? "erreur"
            sqlite3_free(error)
            throw HoldingsDatabaseError.sql(message)
        }
    }

    static func scalarInt(_ db: OpaquePointer, _ sql: String) throws -> Int {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else { throw HoldingsDatabaseError.sql(String(cString: sqlite3_errmsg(db))) }
        defer { sqlite3_finalize(stmt) }
        return sqlite3_step(stmt) == SQLITE_ROW ? Int(sqlite3_column_int64(stmt, 0)) : 0
    }
}
