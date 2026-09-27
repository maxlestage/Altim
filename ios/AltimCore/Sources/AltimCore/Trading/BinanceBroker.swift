import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Exécution spot sur Binance (ou son testnet).
///
/// Sécurité : utilisez une clé API avec **uniquement** la permission « Spot & Margin Trading »,
/// **sans** permission de retrait, et restreinte à votre IP si possible.
public actor BinanceBroker: Broker {
    public nonisolated let name = "Binance"
    public nonisolated let environment: BrokerEnvironment
    let apiKey: String
    let secret: String
    let baseURL: URL
    let transport: HTTPTransport
    let recvWindow = 5_000
    private var timeOffsetMs: Double?
    private var rulesCache: [String: SymbolRules] = [:]

    public init(apiKey: String, secret: String, environment: BrokerEnvironment, transport: HTTPTransport = URLSessionTransport()) {
        self.apiKey = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        self.secret = secret.trimmingCharacters(in: .whitespacesAndNewlines)
        self.environment = environment
        self.transport = transport
        self.baseURL = URL(string: environment == .live ? "https://api.binance.com" : "https://testnet.binance.vision")!
    }

    // MARK: API publique

    public func balances() async throws -> [Balance] {
        let obj = try JSON.object(try await signed("GET", "/api/v3/account", []))
        let rows = obj["balances"] as? [[String: Any]] ?? []
        return rows.compactMap { row in
            guard let asset = row["asset"] as? String, let free = JSON.decimal(row["free"]),
                  let locked = JSON.decimal(row["locked"]), free + locked > 0 else { return nil }
            return Balance(asset: asset, free: free, locked: locked)
        }
    }

    public func rules(for symbol: String) async throws -> SymbolRules {
        if let cached = rulesCache[symbol] { return cached }
        let data = try await request("GET", "/api/v3/exchangeInfo", query: "symbol=\(symbol)", signedRequest: false)
        let rules = try Self.parseRules(data)
        rulesCache[symbol] = rules
        return rules
    }

    public func test(_ order: OrderRequest) async throws {
        try await validate(order)
        _ = try await signed("POST", "/api/v3/order/test", orderParams(order, rules: try await rules(for: order.symbol)))
    }

    public func place(_ order: OrderRequest) async throws -> OrderResult {
        let rules = try await validate(order)
        var notes: [String] = []

        // Une vente peut être bloquée par l'OCO de protection qui réserve les jetons : on l'annule d'abord.
        if order.side == .sell {
            let base = try await balances().first { $0.asset == rules.baseAsset }
            if let base, base.free < order.quantity, base.total >= order.quantity {
                _ = try await signed("DELETE", "/api/v3/openOrders", [("symbol", order.symbol)])
                notes.append("Ordres ouverts (stop/objectif) annulés pour libérer les jetons.")
            }
        }

        let obj = try JSON.object(try await signed("POST", "/api/v3/order", orderParams(order, rules: rules)))
        let orderId = (obj["orderId"] as? NSNumber)?.stringValue ?? "\(obj["orderId"] ?? "?")"
        let status = obj["status"] as? String ?? "UNKNOWN"
        let executed = JSON.decimal(obj["executedQty"]) ?? 0
        let quoteQty = JSON.decimal(obj["cummulativeQuoteQty"]) ?? 0
        let average: Decimal? = executed > 0 ? quoteQty / executed : nil

        var protectionId: String?
        if order.side == .buy, executed > 0, let stop = order.stopLoss, let target = order.takeProfit {
            // Les frais d'achat sont prélevés sur l'actif reçu : on protège la quantité nette.
            let fills = obj["fills"] as? [[String: Any]] ?? []
            let fee = fills.reduce(Decimal(0)) { sum, fill in
                (fill["commissionAsset"] as? String) == rules.baseAsset ? sum + (JSON.decimal(fill["commission"]) ?? 0) : sum
            }
            let protectedQty = rules.normalizeQuantity(executed - fee)
            do {
                let oco = try JSON.object(try await signed("POST", "/api/v3/orderList/oco", [
                    ("symbol", order.symbol),
                    ("side", "SELL"),
                    ("quantity", protectedQty.plainString),
                    ("aboveType", "LIMIT_MAKER"),
                    ("abovePrice", rules.normalizePrice(target).plainString),
                    ("belowType", "STOP_LOSS_LIMIT"),
                    ("belowStopPrice", rules.normalizePrice(stop).plainString),
                    ("belowPrice", rules.normalizePrice(stop * Decimal(string: "0.995")!).plainString),
                    ("belowTimeInForce", "GTC"),
                ]))
                protectionId = (oco["orderListId"] as? NSNumber)?.stringValue
                notes.append("Stop \(stop.plainString) et objectif \(target.plainString) posés (OCO).")
            } catch {
                notes.append("⚠️ Achat exécuté mais protection NON posée : \(error.localizedDescription) Placez un stop manuellement.")
            }
        } else if order.side == .buy, order.stopLoss != nil, executed == 0 {
            notes.append("Ordre limite en attente : posez le stop une fois exécuté.")
        }

        return OrderResult(orderId: orderId, symbol: order.symbol, side: order.side, status: status,
                           executedQuantity: executed, averagePrice: average, protectionOrderId: protectionId,
                           isSimulated: false, notes: notes)
    }

    // MARK: Interne

    @discardableResult
    private func validate(_ order: OrderRequest) async throws -> SymbolRules {
        guard !apiKey.isEmpty, !secret.isEmpty else { throw APIError.missingCredentials }
        let rules = try await rules(for: order.symbol)
        let reference: Decimal
        if let limit = order.limitPrice { reference = limit } else { reference = try await lastPrice(order.symbol) }
        let issues = rules.issues(for: order, referencePrice: reference)
        if !issues.isEmpty { throw BrokerError.validation(issues) }
        return rules
    }

    private func lastPrice(_ symbol: String) async throws -> Decimal {
        let obj = try JSON.object(try await request("GET", "/api/v3/ticker/price", query: "symbol=\(symbol)", signedRequest: false))
        guard let price = JSON.decimal(obj["price"]) else { throw APIError.decoding("prix Binance") }
        return price
    }

    func orderParams(_ order: OrderRequest, rules: SymbolRules) -> [(String, String)] {
        var params: [(String, String)] = [
            ("symbol", order.symbol),
            ("side", order.side.rawValue),
            ("type", order.type.rawValue),
            ("quantity", order.quantity.plainString),
            ("newOrderRespType", "FULL"),
            ("newClientOrderId", "altim-" + UUID().uuidString.prefix(18).replacingOccurrences(of: "-", with: "")),
        ]
        if order.type == .limit, let price = order.limitPrice {
            params.append(("timeInForce", "GTC"))
            params.append(("price", rules.normalizePrice(price).plainString))
        }
        return params
    }

    static func parseRules(_ data: Data) throws -> SymbolRules {
        let obj = try JSON.object(data)
        guard let s = (obj["symbols"] as? [[String: Any]])?.first,
              let symbol = s["symbol"] as? String,
              let filters = s["filters"] as? [[String: Any]] else { throw APIError.decoding("règles Binance") }
        func filter(_ type: String) -> [String: Any]? { filters.first { $0["filterType"] as? String == type } }
        let lot = filter("LOT_SIZE")
        let price = filter("PRICE_FILTER")
        let notional = filter("NOTIONAL") ?? filter("MIN_NOTIONAL")
        return SymbolRules(
            symbol: symbol,
            baseAsset: s["baseAsset"] as? String ?? "",
            quoteAsset: s["quoteAsset"] as? String ?? "",
            stepSize: JSON.decimal(lot?["stepSize"]) ?? Decimal(string: "0.00000001")!,
            minQuantity: JSON.decimal(lot?["minQty"]) ?? 0,
            maxQuantity: JSON.decimal(lot?["maxQty"]) ?? 0,
            tickSize: JSON.decimal(price?["tickSize"]) ?? Decimal(string: "0.00000001")!,
            minNotional: JSON.decimal(notional?["minNotional"]) ?? 0,
            isTradable: (s["status"] as? String) == "TRADING"
        )
    }

    static func encode(_ params: [(String, String)]) -> String {
        var allowed = CharacterSet.urlQueryAllowed
        allowed.remove(charactersIn: "&=+?")
        return params.map { "\($0.0)=\($0.1.addingPercentEncoding(withAllowedCharacters: allowed) ?? $0.1)" }
            .joined(separator: "&")
    }

    private func serverOffset() async throws -> Double {
        if let timeOffsetMs { return timeOffsetMs }
        let before = Date().timeIntervalSince1970 * 1000
        let obj = try JSON.object(try await request("GET", "/api/v3/time", query: nil, signedRequest: false))
        let after = Date().timeIntervalSince1970 * 1000
        let server = JSON.double(obj["serverTime"]) ?? after
        let offset = server - (before + after) / 2
        timeOffsetMs = offset
        return offset
    }

    private func signed(_ method: String, _ path: String, _ params: [(String, String)]) async throws -> Data {
        let offset = try await serverOffset()
        let timestamp = Int64(Date().timeIntervalSince1970 * 1000 + offset)
        let query = Self.encode(params + [("recvWindow", String(recvWindow)), ("timestamp", String(timestamp))])
        let signature = Signer.hmacSHA256Hex(key: secret, message: query)
        return try await request(method, path, query: query + "&signature=" + signature, signedRequest: true)
    }

    private func request(_ method: String, _ path: String, query: String?, signedRequest: Bool) async throws -> Data {
        var urlString = baseURL.absoluteString + path
        if let query { urlString += "?" + query }
        guard let url = URL(string: urlString) else { throw APIError.invalidResponse }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if signedRequest { request.setValue(apiKey, forHTTPHeaderField: "X-MBX-APIKEY") }
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await transport.send(request)
        if response.statusCode == 451 { throw APIError.unavailableInRegion }
        guard (200..<300).contains(response.statusCode) else {
            if let obj = try? JSON.object(data), let code = obj["code"] as? Int {
                if code == -1021 { timeOffsetMs = nil }
                throw BrokerError.rejected(code: code, message: obj["msg"] as? String ?? "")
            }
            throw APIError.http(status: response.statusCode, message: String(data: data, encoding: .utf8) ?? "")
        }
        return data
    }
}
