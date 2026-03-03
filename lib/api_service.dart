import 'dart:convert';
import 'package:http/http.dart' as http;
import 'dart:async';

class ApiService {
  final String baseUrl;
  String? accessToken;

  ApiService({required this.baseUrl});

  Future<Map<String, dynamic>> showConfig() async {
    return await _authenticatedGet('/show_config');
  }

  static Future<bool> ping(String baseUrl) async {
    try {
      // We try to reach the /ping endpoint with a short timeout.
      final response = await http.get(
        Uri.parse('$baseUrl/ping'),
      ).timeout(const Duration(seconds: 3)); // 3-second timeout

      // If we get a 200 OK and the body is correct, the bot is online.
      if (response.statusCode == 200 && jsonDecode(response.body)['status'] == 'pong') {
        return true;
      }
      return false;
    } on TimeoutException {
      // If the request times out, the bot is offline.
      return false;
    } on Exception {
      // Any other error (connection refused, DNS error, etc.) means offline.
      return false;
    }
  }

  Future<String> login(String username, String password) async {
    final response = await http.post(
      Uri.parse('$baseUrl/token/login'),
      headers: {
        'Authorization': 'Basic ${base64Encode(utf8.encode('$username:$password'))}',
      },
    ).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      final data = jsonDecode(response.body);
      accessToken = data['access_token'];
      return accessToken!; // Return the token
    } else {
      throw Exception('Login failed: ${response.body}');
    }
  }

  Future<Map<String, dynamic>> _authenticatedGet(String endpoint) async {
    if (accessToken == null) throw Exception('Not authenticated');

    final response = await http.get(
      Uri.parse('$baseUrl$endpoint'),
      headers: {'Authorization': 'Bearer $accessToken'},
    ).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      return jsonDecode(response.body);
    } else {
      throw Exception('Request failed: ${response.body}');
    }
  }

  Future<List<dynamic>> getOpenTrades() async {
    if (accessToken == null) throw Exception('Not authenticated');

    final response = await http.get(
      Uri.parse('$baseUrl/status'),
      headers: {'Authorization': 'Bearer $accessToken'},
    ).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      final data = jsonDecode(response.body);
      if (data is List) {
        return data;
      } else {
        throw Exception('Unexpected response format: Expected a List for open trades.');
      }
    } else {
      throw Exception('Request failed: ${response.body}');
    }
  }

  Future<List<dynamic>> getClosedTrades({int limit = 50, int offset = 0}) async {
    final data = await _authenticatedGet('/trades?limit=$limit&offset=$offset');
    if (data['trades'] is List) {
      return data['trades'];
    } else {
      throw Exception('Unexpected response format: "trades" key is not a list');
    }
  }

  Future<Map<String, dynamic>> getProfitSummary() async {
    return await _authenticatedGet('/profit');
  }

  Future<Map<String, dynamic>> getBalance() async {
    return await _authenticatedGet('/balance');
  }

  Future<Map<String, dynamic>> _authenticatedPost(String endpoint, Map<String, dynamic> body) async {
    if (accessToken == null) throw Exception('Not authenticated');

    final response = await http.post(
      Uri.parse('$baseUrl$endpoint'),
      headers: {
        'Authorization': 'Bearer $accessToken',
        'Content-Type': 'application/json',
      },
      body: jsonEncode(body),
    ).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      return jsonDecode(response.body);
    } else {
      throw Exception('Request failed: ${response.body}');
    }
  }

  Future<Map<String, dynamic>> forceExit(String tradeId, String orderType) async {
    return await _authenticatedPost('/forceexit', {
      'tradeid': tradeId,
      'ordertype': orderType,
    });
  }

  Future<List<dynamic>> getLogs({int limit = 500}) async {
    final data = await _authenticatedGet('/logs?limit=$limit');
    if (data['logs'] is List) {
      return data['logs'];
    }
    return [];
  }

  Future<List<String>> getWhitelist() async {
    final data = await _authenticatedGet('/whitelist');
    final list = data['whitelist'];
    if (list is List) return list.map((e) => e.toString()).toList();
    return [];
  }

  Future<Map<String, dynamic>> getPairCandles(String pair, String timeframe, {int limit = 300}) async {
    final encoded = Uri.encodeComponent(pair);
    return await _authenticatedGet('/pair_candles?pair=$encoded&timeframe=$timeframe&limit=$limit');
  }
}
