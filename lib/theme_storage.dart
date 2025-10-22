import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';


class ThemeStorage {
  static const _key = 'theme_mode';

  // Reads the saved theme mode from storage. Defaults to system theme.
  static Future<ThemeMode> getThemeMode() async {
    final prefs = await SharedPreferences.getInstance();
    final themeString = prefs.getString(_key);
    switch (themeString) {
      case 'light':
        return ThemeMode.light;
      case 'dark':
        return ThemeMode.dark;
      default:
        return ThemeMode.system;
    }
  }

  // Saves the chosen theme mode to storage.
  static Future<void> saveThemeMode(ThemeMode mode) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_key, mode.name);
  }
}
