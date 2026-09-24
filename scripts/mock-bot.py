#!/usr/bin/env python3
"""Minimal fake Freqtrade, so the UI can be exercised without a real bot."""
import json, math, random
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs

random.seed(7)
N = 327
BASE = 1_780_000_000_000  # epoch ms

# A plausible equity curve: mostly small wins, occasional larger losses.
TRADES = []
for i in range(1, N + 1):
    win = random.random() < 0.58
    profit = round(random.uniform(0.4, 4.2) if win else -random.uniform(0.3, 3.1), 4)
    close = BASE + i * 7_200_000 + random.randint(0, 3_000_000)
    TRADES.append({
        "trade_id": i, "pair": random.choice(["ETH/USDT:USDT", "SOL/USDT:USDT", "BTC/USDT:USDT"]),
        "is_open": False, "is_short": random.random() < 0.3,
        "stake_amount": 100.0, "open_rate": round(random.uniform(100, 3000), 4),
        "close_rate": round(random.uniform(100, 3000), 4),
        "profit_ratio": round(profit / 100, 5), "profit_abs": profit,
        "open_timestamp": close - 7_200_000, "close_timestamp": close,
        "exit_reason": "roi" if win else "stop_loss",
    })
TOTAL = round(sum(t["profit_abs"] for t in TRADES), 4)
WINS = sum(1 for t in TRADES if t["profit_abs"] > 0)

OPEN = [{
    "trade_id": N + 1, "pair": "ETH/USDT:USDT", "is_open": True, "is_short": False,
    "stake_amount": 100.0, "open_rate": 2500.0, "current_rate": 2561.6,
    "profit_ratio": 0.0616, "profit_abs": 6.16,
    "open_timestamp": BASE + N * 7_200_000,
}]

def route(path, query):
    if path.endswith("/ping"): return {"status": "pong"}
    if path.endswith("/token/login"): return {"access_token": "a", "refresh_token": "r"}
    if path.endswith("/token/refresh"): return {"access_token": "a2"}
    if path.endswith("/show_config"):
        return {"version": "2026.4", "dry_run": True, "trading_mode": "futures",
                "short_allowed": True, "stake_currency": "USDT", "stake_amount": "unlimited",
                "max_open_trades": 2.0, "stoploss": -0.1, "stoploss_on_exchange": False,
                "timeframe": "2h", "exchange": "binance", "strategy": "SMCStrategy2h",
                "bot_name": "mock", "state": "running", "runmode": "dry_run"}
    if path.endswith("/status"): return OPEN
    if path.endswith("/trades"):
        limit = int(query.get("limit", ["50"])[0]); offset = int(query.get("offset", ["0"])[0])
        page = TRADES[offset:offset + limit]
        return {"trades": page, "trades_count": len(page), "offset": offset, "total_trades": N}
    if path.endswith("/profit"):
        return {"profit_closed_coin": TOTAL, "profit_closed_percent_mean": round(TOTAL / N, 3),
                "closed_trade_count": N, "trade_count": N + 1,
                "best_pair": "SOL/USDT:USDT", "best_pair_profit_ratio": 0.051,
                "winning_trades": WINS, "losing_trades": N - WINS,
                "profit_factor": 1.74, "trading_volume": 32700.0,
                "max_drawdown": 0.0863, "avg_duration": "2:14:33"}
    if path.endswith("/balance"):
        return {"total": 1198.9, "stake": "USDT", "starting_capital": 1000.0,
                "currencies": [{"currency": "USDT", "free": 998.9, "used": 200.0, "is_position": False}]}
    if path.endswith("/whitelist"):
        return {"whitelist": ["ETH/USDT:USDT", "SOL/USDT:USDT", "BTC/USDT:USDT"], "length": 3}
    if path.endswith("/logs"):
        return {"log_count": 3, "logs": [
            [f"2026-09-24 08:00:0{i}", 1789516800 + i, "freqtrade.worker",
             ["INFO", "WARNING", "ERROR"][i % 3], f"mock log line {i}"] for i in range(3)]}
    if path.endswith("/pair_candles"):
        cols = ["date", "open", "high", "low", "close", "volume", "__date_ts"]
        rows = []
        for i in range(300):
            t = BASE + i * 7_200_000
            c = 2500 + 120 * math.sin(i / 14)
            rows.append([None, c - 4, c + 9, c - 9, round(c, 3), 11.0, t])
        return {"pair": "ETH/USDT:USDT", "timeframe": "2h", "columns": cols, "data": rows, "length": len(rows)}
    return None

class H(BaseHTTPRequestHandler):
    def _send(self, body):
        payload = json.dumps(body).encode()
        self.send_response(200 if body is not None else 404)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers(); self.wfile.write(payload)
    def do_GET(self):
        u = urlparse(self.path); self._send(route(u.path, parse_qs(u.query)))
    def do_POST(self):
        u = urlparse(self.path); self._send(route(u.path, parse_qs(u.query)))
    def log_message(self, *a): pass

HTTPServer(("127.0.0.1", 8899), H).serve_forever()
