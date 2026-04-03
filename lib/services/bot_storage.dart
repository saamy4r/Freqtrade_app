import 'dart:convert';
import 'package:shared_preferences/shared_preferences.dart';
import '../models/bot.dart';

class BotStorage {
  static const _key = 'bots_list';

  static Future<List<Bot>> getBots() async {
    final prefs = await SharedPreferences.getInstance();
    final List<String> botsJson = prefs.getStringList(_key) ?? [];
    return botsJson.map((botString) => Bot.fromJson(jsonDecode(botString))).toList();
  }

  static Future<void> saveBots(List<Bot> bots) async {
    final prefs = await SharedPreferences.getInstance();
    final List<String> botsJson = bots.map((bot) => jsonEncode(bot.toJson())).toList();
    await prefs.setStringList(_key, botsJson);
  }

  static Future<void> addBot(Bot newBot) async {
    final bots = await getBots();
    bots.add(newBot);
    await saveBots(bots);
  }

  static Future<void> deleteBot(String botId) async {
    final bots = await getBots();
    bots.removeWhere((bot) => bot.id == botId);
    await saveBots(bots);
  }
}
