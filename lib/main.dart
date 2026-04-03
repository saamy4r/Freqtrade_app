import 'package:flutter/material.dart';
import 'services/theme_storage.dart';
import 'screens/app_shell.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  ThemeMode _themeMode = ThemeMode.system;

  @override
  void initState() {
    super.initState();
    _loadTheme();
  }

  Future<void> _loadTheme() async {
    final savedTheme = await ThemeStorage.getThemeMode();
    if (mounted) {
      setState(() {
        _themeMode = savedTheme;
      });
    }
  }

  void _changeTheme(ThemeMode newMode) {
    setState(() {
      _themeMode = newMode;
      ThemeStorage.saveThemeMode(newMode);
    });
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Freqtrade Visualizer',
      theme: ThemeData(
        brightness: Brightness.light,
        primarySwatch: Colors.blue,
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        brightness: Brightness.dark,
        primarySwatch: Colors.blue,
        useMaterial3: true,
      ),
      themeMode: _themeMode,
      home: AppShell(
        themeMode: _themeMode,
        onThemeChanged: _changeTheme,
      ),
    );
  }
}
