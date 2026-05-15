import 'dart:convert';
import 'dart:io';
import 'package:path_provider/path_provider.dart';

class BotCacheService {
  static Future<File> _cacheFile(String botId) async {
    final dir = await getApplicationDocumentsDirectory();
    final cacheDir = Directory('${dir.path}/bot_cache');
    if (!await cacheDir.exists()) await cacheDir.create();
    return File('${cacheDir.path}/$botId.json');
  }

  static Future<Map<String, dynamic>?> load(String botId) async {
    try {
      final file = await _cacheFile(botId);
      if (!await file.exists()) return null;
      return jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    } catch (_) {
      return null;
    }
  }

  // Merges partial data into the existing cache, updating lastSynced.
  static Future<void> merge(String botId, Map<String, dynamic> partial) async {
    try {
      final file = await _cacheFile(botId);
      Map<String, dynamic> data = {};
      if (await file.exists()) {
        data = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
      }
      data.addAll(partial);
      data['lastSynced'] = DateTime.now().toIso8601String();
      await file.writeAsString(jsonEncode(data));
    } catch (_) {}
  }

  static Future<void> delete(String botId) async {
    try {
      final file = await _cacheFile(botId);
      if (await file.exists()) await file.delete();
    } catch (_) {}
  }
}
