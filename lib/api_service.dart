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
    );

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
    );

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
    );

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
}
