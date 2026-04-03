import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';
import '../models/bot.dart';
import '../services/api_service.dart';
import '../services/bot_storage.dart';
import 'bots_screen.dart';
import 'open_trades_screen.dart';
import 'closed_trades_screen.dart';
import 'dashboard_screen.dart';
import 'chart_screen.dart';
import 'logs_screen.dart';

class AppShell extends StatefulWidget {
  final ThemeMode themeMode;
  final Function(ThemeMode) onThemeChanged;

  const AppShell({
    super.key,
    required this.themeMode,
    required this.onThemeChanged,
  });

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  List<Bot> _bots = [];
  Bot? _activeBot;
  ApiService? _apiService;
  bool? _isDryRun;
  int _selectedIndex = 0;
  bool _isConnecting = false;
  static const _activeBotKey = 'active_bot_id';

  @override
  void initState() {
    super.initState();
    _loadBots();
  }

  Future<void> _loadBots() async {
    if (mounted) setState(() => _isConnecting = true);
    final bots = await BotStorage.getBots();
    if (mounted) {
      setState(() {
        _bots = bots;
      });
      if (_bots.isNotEmpty) {
        final prefs = await SharedPreferences.getInstance();
        final savedBotId = prefs.getString(_activeBotKey);

        Bot? botToLoad;
        if (savedBotId != null) {
          try {
            botToLoad = _bots.firstWhere((b) => b.id == savedBotId);
          } catch (e) {
            botToLoad = null;
          }
        }

        await _selectBot(botToLoad ?? _bots.first);
      } else {
        setState(() => _isConnecting = false);
      }
    }
  }

  Future<void> _selectBot(Bot bot) async {
    if (mounted) {
      setState(() {
        _activeBot = bot;
        _isConnecting = true;
        _isDryRun = null;
        _selectedIndex = 0;
      });
    }
    try {
      final apiService = ApiService(baseUrl: bot.url);
      await apiService.login(bot.username, bot.password);
      final config = await apiService.showConfig();
      final isDryRun = config['dry_run'] as bool?;
      if (mounted) {
        setState(() {
          _apiService = apiService;
          _isDryRun = isDryRun;
          _isConnecting = false;
        });
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to connect to ${bot.name}: $e')),
        );
        setState(() {
          _activeBot = null;
          _apiService = null;
          _isDryRun = null;
          _isConnecting = false;
        });
      }
    }
  }

  Future<void> _userSelectBot(Bot bot) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_activeBotKey, bot.id);
    } catch (_) {}
    await _selectBot(bot);
  }

  Future<void> _addBot() async {
    final newBot = await Navigator.push<Bot>(
      context,
      MaterialPageRoute(builder: (_) => const LoginScreen()),
    );
    if (newBot != null) {
      await BotStorage.addBot(newBot);
      final allBots = await BotStorage.getBots();
      if (mounted) {
        setState(() { _bots = allBots; });
        await _userSelectBot(newBot);
      }
    }
  }

  Future<void> _deleteBot(String botId) async {
    final wasActiveBot = _activeBot?.id == botId;

    if (wasActiveBot) {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(_activeBotKey);
    }

    await BotStorage.deleteBot(botId);
    final allBots = await BotStorage.getBots();

    if (mounted) {
      setState(() { _bots = allBots; });
      if (_bots.isEmpty) {
        setState(() {
          _activeBot = null;
          _apiService = null;
        });
      } else if (wasActiveBot) {
        await _userSelectBot(_bots.first);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_activeBot == null && !_isConnecting) {
      return BotsScreen(
        bots: _bots,
        activeBot: _activeBot,
        onAddBot: _addBot,
        onSelectBot: _selectBot,
        onDeleteBot: _deleteBot,
      );
    }

    final screens = [
      if (_apiService != null) OpenTradesScreen(apiService: _apiService!),
      if (_apiService != null) ClosedTradesScreen(apiService: _apiService!),
      if (_apiService != null) DashboardScreen(apiService: _apiService!),
      if (_apiService != null) ChartScreen(apiService: _apiService!),
      if (_apiService != null) LogsScreen(apiService: _apiService!),
      BotsScreen(
        bots: _bots,
        activeBot: _activeBot,
        onAddBot: _addBot,
        onSelectBot: _userSelectBot,
        onDeleteBot: _deleteBot,
      ),
    ];

    return Scaffold(
      appBar: AppBar(
        title: Text(_activeBot?.name ?? 'Connecting...'),
        actions: [
          if (_isDryRun != null && !_isConnecting)
            Padding(
              padding: const EdgeInsets.only(right: 8.0),
              child: Center(
                child: Text(
                  _isDryRun! ? 'DRY' : 'LIVE',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    color: _isDryRun! ? Colors.green : Colors.blue,
                    fontSize: 16,
                  ),
                ),
              ),
            ),
          if (_isConnecting)
            const Padding(
              padding: EdgeInsets.only(right: 16.0),
              child: SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 3, color: Colors.white)),
            ),
          IconButton(
            icon: Icon(widget.themeMode == ThemeMode.dark ? Icons.light_mode : Icons.dark_mode),
            onPressed: () {
              final newMode = widget.themeMode == ThemeMode.dark ? ThemeMode.light : ThemeMode.dark;
              widget.onThemeChanged(newMode);
            },
          ),
        ],
      ),
      body: _isConnecting
          ? const Center(child: CircularProgressIndicator())
          : IndexedStack(index: _selectedIndex, children: screens),
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _selectedIndex,
        onTap: (index) => setState(() => _selectedIndex = index),
        type: BottomNavigationBarType.fixed,
        items: const [
          BottomNavigationBarItem(icon: Icon(Icons.open_in_browser), label: 'Open Trades'),
          BottomNavigationBarItem(icon: Icon(Icons.history), label: 'Closed Trades'),
          BottomNavigationBarItem(icon: Icon(Icons.dashboard), label: 'Dashboard'),
          BottomNavigationBarItem(icon: Icon(Icons.candlestick_chart), label: 'Chart'),
          BottomNavigationBarItem(icon: Icon(Icons.terminal), label: 'Logs'),
          BottomNavigationBarItem(icon: Icon(Icons.smart_toy), label: 'Bots'),
        ],
      ),
    );
  }
}
