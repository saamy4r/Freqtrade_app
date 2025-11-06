import 'dart:async'; 
import 'dart:io'; 
import 'package:flutter/material.dart';
import 'package:uuid/uuid.dart';
import 'package:intl/intl.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'api_service.dart';
import 'bot_model.dart';
import 'bot_storage.dart';
import 'theme_storage.dart';

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
            // Try to find the bot with the saved ID
            botToLoad = _bots.firstWhere((b) => b.id == savedBotId);
          } catch (e) {
            // If not found (e.g., bot was deleted), botToLoad will remain null
            botToLoad = null;
          }
        }
        
        // If we found the bot, load it. Otherwise, load the first bot.
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
    } catch (e) {
      print("Failed to save active bot ID: $e");
    }
    // Now, call the main selection logic
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
      if(mounted) {
        setState(() { _bots = allBots; });
        await _userSelectBot(newBot);
      }
    }
  }

  Future<void> _deleteBot(String botId) async {
    final wasActiveBot = _activeBot?.id == botId;
    
    // If we are deleting the active bot, clear the saved preference
    if (wasActiveBot) {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(_activeBotKey);
    }

    await BotStorage.deleteBot(botId);
    final allBots = await BotStorage.getBots();
    
    if(mounted) {
      setState(() { _bots = allBots; });
      if (_bots.isEmpty) {
        setState(() {
          _activeBot = null;
          _apiService = null;
        });
      } else if (wasActiveBot) {
        // If the active bot was deleted, load the first one in the list
        await _userSelectBot(_bots.first);
      }
      // If a non-active bot was deleted, we don't need to do anything else
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
          BottomNavigationBarItem(icon: Icon(Icons.smart_toy), label: 'Bots'),
        ],
      ),
    );
  }
}

// BotsScreen: To manage adding, selecting, and deleting bots.
class BotsScreen extends StatefulWidget {
  final List<Bot> bots;
  final Bot? activeBot;
  final VoidCallback onAddBot;
  final Function(Bot) onSelectBot;
  final Function(String) onDeleteBot;

  const BotsScreen({
    super.key,
    required this.bots,
    required this.activeBot,
    required this.onAddBot,
    required this.onSelectBot,
    required this.onDeleteBot,
  });

  @override
  State<BotsScreen> createState() => _BotsScreenState();
}

class _BotsScreenState extends State<BotsScreen> {
  // State to hold the online status, mapping Bot ID to true/false
  Map<String, bool> _onlineStatus = {};
  bool _isLoadingStatus = true;

  @override
  void initState() {
    super.initState();
    // When the screen loads, check the status of all bots
    _checkAllBotStatus();
  }

  // This function pings all bots concurrently to check their status
  Future<void> _checkAllBotStatus() async {
    if (!mounted) return;
    setState(() {
      _isLoadingStatus = true;
    });

    // Create a list of ping Futures for all bots
    final pings = widget.bots.map((bot) => ApiService.ping(bot.url)).toList();

    // Await all of them at once
    final results = await Future.wait(pings);

    // Create a map from bot ID to its online status result
    final statusMap = Map.fromIterables(
      widget.bots.map((bot) => bot.id),
      results,
    );

    if (mounted) {
      setState(() {
        _onlineStatus = statusMap;
        _isLoadingStatus = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.bots.isEmpty) {
      return Scaffold(
        appBar: AppBar(title: const Text("Manage Bots")),
        body: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Text("No bots found."),
              const SizedBox(height: 20),
              ElevatedButton.icon(
                icon: const Icon(Icons.add),
                label: const Text("Add Your First Bot"),
                onPressed: widget.onAddBot,
              ),
            ],
          ),
        ),
      );
    }

    return ListView.builder(
      itemCount: widget.bots.length + 1,
      itemBuilder: (context, index) {
        if (index == widget.bots.length) {
          return Padding(
            padding: const EdgeInsets.all(16.0),
            child: OutlinedButton.icon(
              icon: const Icon(Icons.add),
              label: const Text("Add Another Bot"),
              onPressed: widget.onAddBot,
            ),
          );
        }

        final bot = widget.bots[index];
        final bool isActive = bot.id == widget.activeBot?.id;

        final bool? isOnline = _onlineStatus[bot.id];
        Color statusColor;
        if (_isLoadingStatus) {
          statusColor = Colors.grey; // Grey while loading
        } else if (isOnline == true) {
          statusColor = Colors.green; // Green for online
        } else {
          statusColor = Colors.red; // Red for offline
        }

        // The dot widget
        final statusDot = Container(
          width: 12,
          height: 12,
          decoration: BoxDecoration(
            color: statusColor,
            shape: BoxShape.circle,
          ),
        );

        return Card(
          margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          child: ListTile(
            leading: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                statusDot,
                const SizedBox(width: 12),
                Icon(Icons.smart_toy_outlined, color: isActive ? Theme.of(context).primaryColor : Colors.grey),
              ],
            ),
            title: Text(bot.name, style: TextStyle(fontWeight: isActive ? FontWeight.bold : FontWeight.normal)),
            subtitle: Text(bot.url, overflow: TextOverflow.ellipsis),
            trailing: IconButton(
              icon: const Icon(Icons.delete_outline, color: Colors.red),
              onPressed: () => widget.onDeleteBot(bot.id),
            ),
            onTap: () => widget.onSelectBot(bot),
            tileColor: isActive ? Theme.of(context).primaryColor.withOpacity(0.1) : null,
          ),
        );
      },
    );
  }
}

// LoginScreen: Used for adding new bots.
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key});

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _botNameController = TextEditingController();
  final _urlController = TextEditingController();
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  bool _isLoading = false;
  String? _errorMessage;

  Future<void> _testAndSave() async {
    if (_botNameController.text.isEmpty || _urlController.text.isEmpty || _usernameController.text.isEmpty) {
      setState(() {
        _errorMessage = 'All fields are required.'; // <-- USE THE VARIABLE
      });
      return;
    }

    setState(() {
      _isLoading = true;
      _errorMessage = null; // <-- Clear previous errors
    });

    try {
      String userInputUrl = _urlController.text.trim();
      if (userInputUrl.endsWith('/')) {
        userInputUrl = userInputUrl.substring(0, userInputUrl.length - 1);
      }
      if (!userInputUrl.endsWith('/api/v1')) {
        userInputUrl = '$userInputUrl/api/v1';
      }

      final apiService = ApiService(baseUrl: userInputUrl);
      await apiService.login(
        _usernameController.text,
        _passwordController.text,
      );

      final newBot = Bot(
        id: const Uuid().v4(),
        name: _botNameController.text.trim(),
        url: userInputUrl,
        username: _usernameController.text,
        password: _passwordController.text,
      );

      if (mounted) {
        Navigator.pop(context, newBot);
      }

    } on TimeoutException { // <-- CATCH SPECIFIC ERRORS
      if (mounted) {
        setState(() {
          _errorMessage = 'Connection timed out. Check the URL or port.';
        });
      }
    } on SocketException { // <-- CATCH SPECIFIC ERRORS
      if (mounted) {
        setState(() {
          _errorMessage = 'Connection failed. Check the URL or port.';
        });
      }
    } on Exception catch (e) { // <-- CATCH ALL OTHER ERRORS
      if (mounted) {
        setState(() {
          // Check for the 'Login failed' text from our api_service
          if (e.toString().contains('Login failed')) {
            _errorMessage = 'Login failed. Check username or password.';
          } else {
            // Show a generic error for other issues
            _errorMessage = 'An unknown error occurred.';
          }
        });
      }
    } finally {
      if(mounted) {
        setState(() => _isLoading = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Add a New Bot')),
      body: SingleChildScrollView(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            children: [
              TextField(
                controller: _botNameController,
                decoration: const InputDecoration(labelText: 'Bot Name (e.g., My ETH Bot)'),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _urlController,
                decoration: const InputDecoration(
                  labelText: 'URL',
                  hintText: 'e.g., http://192.168.1.10:8080',
                ),
                keyboardType: TextInputType.url,
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _usernameController,
                decoration: const InputDecoration(labelText: 'Username'),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: _passwordController,
                obscureText: true,
                decoration: const InputDecoration(labelText: 'Password'),
              ),
              const SizedBox(height: 20),
              if (_errorMessage != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 16.0),
                  child: Text(
                    _errorMessage!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                      fontWeight: FontWeight.bold,
                    ),
                    textAlign: TextAlign.center,
                  ),
                ),
              _isLoading
                  ? const CircularProgressIndicator()
                  : ElevatedButton(onPressed: _testAndSave, child: const Text('Login')),
            ],
          ),
        ),
      ),
    );
  }
}

// OpenTradesScreen: Shows detailed info for open trades.
class OpenTradesScreen extends StatefulWidget {
  final ApiService apiService;
  const OpenTradesScreen({super.key, required this.apiService});

  @override
  State<OpenTradesScreen> createState() => _OpenTradesScreenState();
}

class _OpenTradesScreenState extends State<OpenTradesScreen> with AutomaticKeepAliveClientMixin {
  List<dynamic>? _trades;
  Map<String, dynamic>? _balance;
  double? _freeBalance;
  double? _stakedAmount;
  String? _error;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _fetchData();
  }

  @override
  void didUpdateWidget(covariant OpenTradesScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.apiService.baseUrl != oldWidget.apiService.baseUrl) {
      _fetchData();
    }
  }

  Future<void> _fetchData() async {
    setState(() {
      _trades = null;
      _balance = null;
      _freeBalance = null;
      _stakedAmount = null;
      _error = null;
    });

    try {
      final results = await Future.wait([
        widget.apiService.getOpenTrades(),
          widget.apiService.getBalance(),
        ]);
        if (!mounted) return;
        final tradesResult = results[0] as List<dynamic>;    // This is 'result[0]'
        final balanceResult = results[1] as Map<String, dynamic>; // This is 'result[1]'
        double freeBalance = 0.0;
        double stakedAmount = 0.0;
        try {
          if (balanceResult['currencies'] is List) {
            final currencies = balanceResult['currencies'] as List<dynamic>;
            
            // Find the stake currency data (the one that is not a position)
            final stakeCurrencyData = currencies.firstWhere(
              (c) => c['is_position'] == false,
              orElse: () => null,
            );

            if (stakeCurrencyData != null) {
              freeBalance = stakeCurrencyData['free']?.toDouble() ?? 0.0;
              stakedAmount = stakeCurrencyData['used']?.toDouble() ?? 0.0;
            }
          }
      } catch (e) {
        print('Error parsing free balance in OpenTrades: $e');
      }
        // --- ADD THESE TWO LINES ---
        print('--- DEBUG: /balance RESPONSE ---');
        print(balanceResult);
        tradesResult.sort((a, b) {
          final dateA = a['open_date'] ?? '';
          final dateB = b['open_date'] ?? '';
          return dateB.compareTo(dateA); // Compare B to A for descending
        });
        setState(() {
          _trades = tradesResult;
          _balance = balanceResult;
          _freeBalance = freeBalance;
          _stakedAmount = stakedAmount;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
      });
    }
  }

  Widget _buildInfoColumn(String title, String value, {CrossAxisAlignment? alignment}) {
    return Column(
      crossAxisAlignment: alignment ?? CrossAxisAlignment.start,
      children: [
        Text(title, style: const TextStyle(color: Colors.grey, fontSize: 12)),
        const SizedBox(height: 2),
        Text(value, style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14)),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);

    if (_error != null) {
      return Center(child: Text('Error: $_error\nPull down to refresh.'));
    }
    if (_trades == null || _balance == null || _freeBalance == null || _stakedAmount == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final totalOpenProfit = _trades!.fold<double>(
      0.0,
          (sum, trade) => sum + (trade['profit_abs'] ?? 0.0),
    );
    // GET CURRENCY AND BALANCE FROM THE NEW _balance STATE
    final currency = _balance!['stake_currency'] ?? '';
    
    // This is the cash balance (e.g., 1686.13)
    final cashBalance = _balance!['total']?.toDouble() ?? 0.0;
    
    // This is the true free balance we just found (e.g., 16.86)
    final double freeBalance = _freeBalance!; 
    final double stakedAmount = _stakedAmount!;
    // This is the correct final portfolio value (e.g., 1410.95)
    final totalPortfolioValue = cashBalance + totalOpenProfit;

    return Column(
      children: [
        Card(
          margin: const EdgeInsets.fromLTRB(12, 12, 12, 0),
          elevation: 2,
          child: ListTile(
            leading: Icon(
              Icons.account_balance_wallet_outlined,
              color: Theme.of(context).primaryColor,
            ),
            title: const Text('Total Portfolio Value', style: TextStyle(fontWeight: FontWeight.bold)),
            trailing: Text(
              '${totalPortfolioValue.toStringAsFixed(2)} USDT',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                fontSize: 16,
                color: Theme.of(context).textTheme.bodyLarge?.color,
              ),
            ),
          ),
        ),
        Card(
          margin: const EdgeInsets.fromLTRB(12, 12, 12, 0),
          elevation: 0,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
            side: BorderSide(color: Colors.grey.withOpacity(0.3), width: 1),
          ),
          child: ListTile(
            leading: Icon(
              Icons.attach_money,
              color: Colors.green,
            ),
            title: const Text('Free / Staked', style: TextStyle(fontWeight: FontWeight.bold)), // <-- CHANGED
            trailing: Text(
              '${freeBalance.toStringAsFixed(2)} / ${stakedAmount.toStringAsFixed(2)} $currency', // <-- CHANGED
              style: TextStyle(
                fontWeight: FontWeight.bold,
                fontSize: 16,
                color: Theme.of(context).textTheme.bodyLarge?.color,
              ),
            ),
          ),
        ),
        Card(
          margin: const EdgeInsets.all(12.0),
          elevation: 2,
          child: ListTile(
            leading: Icon(
              Icons.hourglass_top,
              color: totalOpenProfit >= 0 ? Colors.green : Colors.red,
            ),
            title: const Text('Total Open P/L', style: TextStyle(fontWeight: FontWeight.bold)),
            trailing: Text(
              '${totalOpenProfit.toStringAsFixed(2)} $currency',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                fontSize: 16,
                color: totalOpenProfit >= 0 ? Colors.green : Colors.red,
              ),
            ),
          ),
        ),
        Expanded(
          child: RefreshIndicator(
            onRefresh: _fetchData,
            child: _trades!.isEmpty
                ? const Center(child: Text('No open trades available'))
                : ListView.builder(
              padding: const EdgeInsets.all(8.0),
              itemCount: _trades!.length,
              itemBuilder: (context, index) {
                final trade = _trades![index];
                final profitRatio = trade['profit_ratio'] ?? 0.0;
                final bool isShort = trade['is_short'] ?? false;
                final String tradeDirection = isShort ? 'short' : 'long';
                final stakeAmount = trade['stake_amount'] ?? 0.0;
                final openRate = trade['open_rate'] ?? 0.0;
                final currentRate = trade['current_rate'] ?? 0.0;
                final openDate = trade['open_date'] ?? 'N/A';

                Color cardColor;
                IconData trendIcon;
                Color iconColor;

                if (profitRatio > 0) {
                  cardColor = Colors.green.withOpacity(0.15);
                  trendIcon = Icons.trending_up;
                  iconColor = Colors.green;
                } else if (profitRatio < 0) {
                  cardColor = Colors.red.withOpacity(0.15);
                  trendIcon = Icons.trending_down;
                  iconColor = Colors.red;
                } else {
                  cardColor = Colors.grey.withOpacity(0.1);
                  trendIcon = Icons.trending_flat;
                  iconColor = Colors.grey;
                }

                return Card(
                  color: cardColor,
                  margin: const EdgeInsets.symmetric(horizontal: 4, vertical: 5),
                  elevation: 0,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(10),
                    side: BorderSide(color: Colors.grey.withOpacity(0.2), width: 1),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.all(12.0),
                    child: Column(
                      children: [
                        Row(
                          children: [
                            Text(
                              trade['pair'],
                              style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
                            ),
                            const SizedBox(width: 8),
                              Chip(
                                label: Text(
                                  tradeDirection.toString().toUpperCase(),
                                  style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
                                ),
                                backgroundColor: tradeDirection == 'long' ? Colors.green : Colors.red,
                                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 0),
                                visualDensity: VisualDensity.compact,
                                labelStyle: const TextStyle(fontSize: 10),
                                materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                              ),
                            const Spacer(),
                            Text(
                              '${(profitRatio * 100).toStringAsFixed(2)}%',
                              style: TextStyle(
                                fontWeight: FontWeight.bold,
                                fontSize: 16,
                                color: iconColor,
                              ),
                            ),
                            const SizedBox(width: 4),
                            Icon(trendIcon, color: iconColor, size: 20),
                          ],
                        ),
                        const Divider(height: 16),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            _buildInfoColumn('Stake Amount', '${stakeAmount.toStringAsFixed(2)}'),
                            _buildInfoColumn('Open Price', '${openRate.toStringAsFixed(4)}', alignment: CrossAxisAlignment.center),
                            _buildInfoColumn('Current Price', '${currentRate.toStringAsFixed(4)}', alignment: CrossAxisAlignment.end),
                          ],
                        ),
                        const SizedBox(height: 8),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.start,
                          children: [
                            Text('Opened: $openDate', style: const TextStyle(fontSize: 11, color: Colors.grey)),
                          ],
                        )
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      ],
    );
  }
}


// ClosedTradesScreen: Shows detailed info for closed trades.
class ClosedTradesScreen extends StatelessWidget {
  final ApiService apiService;
  const ClosedTradesScreen({super.key, required this.apiService});

  Widget _buildInfoColumn(String title, String value, {CrossAxisAlignment? alignment}) {
    return Column(
      crossAxisAlignment: alignment ?? CrossAxisAlignment.start,
      children: [
        Text(title, style: const TextStyle(color: Colors.grey, fontSize: 12)),
        const SizedBox(height: 2),
        Text(value, style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14)),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<dynamic>>(
      key: ValueKey(apiService.baseUrl),
      // STEP 1: Update the Future.wait to include getBalance()
      future: Future.wait([
        apiService.getProfitSummary(),
        apiService.getClosedTrades(limit: 500),
        apiService.getBalance(), // <-- This fetches the balance
      ]),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.waiting) {
          return const Center(child: CircularProgressIndicator());
        }
        if (snapshot.hasError) {
          return Center(child: Text('Error: ${snapshot.error}'));
        }
        if (!snapshot.hasData) {
          return const Center(child: Text('Could not load trade data.'));
        }

        // STEP 2: Unpack all the data from the Future
        final profitSummary = snapshot.data![0] as Map<String, dynamic>;
        final trades = snapshot.data![1] as List<dynamic>;
        final balanceData = snapshot.data![2] as Map<String, dynamic>; // <-- This gets the balance data

        trades.sort((a, b) {
          final dateA = a['close_date'] ?? '';
          final dateB = b['close_date'] ?? '';
          return dateB.compareTo(dateA); // Compare B to A for descending order
        });
        
        final totalProfit = profitSummary['profit_closed_coin']?.toDouble() ?? 0.0;
        final currency = profitSummary['stake_currency'] ?? '';
        
        // STEP 3: Use the correct key 'total' (this is the line you asked about)
        final totalPortfolioValue = balanceData['total']?.toDouble() ?? 0.0;

        if (trades.isEmpty) {
          return const Center(child: Text('No closed trades found.'));
        }

        return Column(
          children: [
            // STEP 4: Add the new Card for portfolio value
            Card(
              margin: const EdgeInsets.fromLTRB(12, 12, 12, 0),
              elevation: 2,
              child: ListTile(
                leading: Icon(
                  Icons.account_balance_wallet_outlined,
                  color: Theme.of(context).primaryColor,
                ),
                title: const Text('Current Portfolio Value', style: TextStyle(fontWeight: FontWeight.bold)),
                trailing: Text(
                  '${totalPortfolioValue.toStringAsFixed(2)} USDT',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    fontSize: 16,
                    color: Theme.of(context).textTheme.bodyLarge?.color,
                  ),
                ),
              ),
            ),
            
            // This is your existing card for "Total Closed Profit"
            Card(
              margin: const EdgeInsets.all(12.0),
              elevation: 2,
              child: ListTile(
                leading: Icon(
                  Icons.show_chart,
                  color: totalProfit >= 0 ? Colors.green : Colors.red,
                ),
                title: const Text('Total Closed Profit', style: TextStyle(fontWeight: FontWeight.bold)),
                trailing: Text(
                  '${totalProfit.toStringAsFixed(2)} $currency',
                  style: TextStyle(
                    fontWeight: FontWeight.bold,
                    fontSize: 16,
                    color: totalProfit >= 0 ? Colors.green : Colors.red,
                  ),
                ),
              ),
            ),
            Expanded(
              child: ListView.builder(
                padding: const EdgeInsets.symmetric(horizontal: 8.0),
                itemCount: trades.length,
                itemBuilder: (context, index) {
                  final trade = trades[index];
                  // ... all the rest of your ListView.builder code ...
                  // ... (no changes needed inside here) ...
                  final profitRatio = trade['profit_ratio'] ?? 0.0;
                  final stakeAmount = trade['stake_amount'] ?? 0.0;
                  final openRate = trade['open_rate'] ?? 0.0;
                  final closeRate = trade['close_rate'] ?? 0.0;
                  final openDate = trade['open_date'] ?? 'N/A';
                  final closeDate = trade['close_date'] ?? 'N/A';
                  final bool isShort = trade['is_short'] ?? false;
                  final String tradeDirection = isShort ? 'short' : 'long';

                  Color cardColor;
                  IconData trendIcon;
                  Color iconColor;
                  if (profitRatio > 0) {
                    cardColor = Colors.green.withOpacity(0.15);
                    trendIcon = Icons.trending_up;
                    iconColor = Colors.green;
                  } else if (profitRatio < 0) {
                    cardColor = Colors.red.withOpacity(0.15);
                    trendIcon = Icons.trending_down;
                    iconColor = Colors.red;
                  } else {
                    cardColor = Colors.grey.withOpacity(0.1);
                    trendIcon = Icons.trending_flat;
                    iconColor = Colors.grey;
                  }

                  return Card(
                    color: cardColor,
                    margin: const EdgeInsets.symmetric(horizontal: 4, vertical: 5),
                    elevation: 0,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(10),
                      side: BorderSide(color: Colors.grey.withOpacity(0.2), width: 1),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.all(12.0),
                      child: Column(
                        children: [
                          Row(
                            children: [
                              Text(
                                trade['pair'],
                                style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
                              ),
                              const SizedBox(width: 8),
                                Chip(
                                  label: Text(
                                    tradeDirection.toString().toUpperCase(),
                                    style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
                                  ),
                                  backgroundColor: tradeDirection == 'long' ? Colors.green : Colors.red,
                                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 0),
                                  visualDensity: VisualDensity.compact,
                                  labelStyle: const TextStyle(fontSize: 10),
                                  materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                                ),
                              const Spacer(),
                              Text(
                                '${(profitRatio * 100).toStringAsFixed(2)}%',
                                style: TextStyle(
                                  fontWeight: FontWeight.bold,
                                  fontSize: 16,
                                  color: iconColor,
                                ),
                              ),
                              const SizedBox(width: 4),
                              Icon(trendIcon, color: iconColor, size: 20),
                            ],
                          ),
                          const Divider(height: 16),
                          Row(
                            mainAxisAlignment: MainAxisAlignment.spaceBetween,
                            children: [
                              _buildInfoColumn('Stake Amount', '${stakeAmount.toStringAsFixed(2)}'),
                              _buildInfoColumn('Open Price', '${openRate.toStringAsFixed(4)}', alignment: CrossAxisAlignment.center),
                              _buildInfoColumn('Close Price', '${closeRate.toStringAsFixed(4)}', alignment: CrossAxisAlignment.end),
                            ],
                          ),
                          const SizedBox(height: 8),
                          Row(
                            mainAxisAlignment: MainAxisAlignment.spaceBetween,
                            children: [
                              Text('Opened: $openDate', style: const TextStyle(fontSize: 11, color: Colors.grey)),
                              Text('Closed: $closeDate', style: const TextStyle(fontSize: 11, color: Colors.grey)),
                            ],
                          )
                        ],
                      ),
                    ),
                  );
                },
              ),
            ),
          ],
        );
      },
    );
  }
}


class StatTile extends StatelessWidget {
  final IconData icon;
  final String title;
  final String value;
  final Color? valueColor;

  const StatTile({
    super.key,
    required this.icon,
    required this.title,
    required this.value,
    this.valueColor,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(10),
        side: BorderSide(color: Colors.grey.withOpacity(0.2), width: 1),
      ),
      child: ListTile(
        leading: Icon(icon, size: 28, color: Theme.of(context).colorScheme.primary),
        title: Text(
          value,
          style: TextStyle(
            fontWeight: FontWeight.bold,
            fontSize: 16,
            color: valueColor,
          ),
        ),
        subtitle: Text(
          title,
          style: const TextStyle(color: Colors.grey, fontSize: 12),
        ),
        dense: true,
      ),
    );
  }
}


class DashboardScreen extends StatelessWidget {
  final ApiService apiService;
  const DashboardScreen({super.key, required this.apiService});

  ({List<FlSpot> spots, DateTime? firstDate, DateTime? lastDate, bool isPercentageChart}) _prepareChartData(
      List<dynamic> trades, double startingCapital) {
    if (trades.isEmpty) {
      return (spots: [], firstDate: null, lastDate: null, isPercentageChart: true);
    }
    final sortedTrades = List.from(trades)
      ..sort((a, b) => DateTime.parse(a['close_date']).compareTo(DateTime.parse(b['close_date'])));
    final List<FlSpot> spots = [];
    final firstTradeDate = DateTime.parse(sortedTrades.first['close_date']);
    final bool isPercentageChart = startingCapital > 0;

    double lastYValue = 0.0;
    spots.add(FlSpot(firstTradeDate.millisecondsSinceEpoch.toDouble(), lastYValue));

    double currentCapital = startingCapital;
    double cumulativeAbsoluteProfit = 0.0;

    for (final trade in sortedTrades) {
      final absoluteProfit = trade['profit_abs'] ?? 0.0;
      final closeDate = DateTime.parse(trade['close_date']);
      final timeStamp = closeDate.millisecondsSinceEpoch.toDouble();

      double currentYValue;
      if (isPercentageChart) {
        currentCapital += absoluteProfit;
        currentYValue = ((currentCapital - startingCapital) / startingCapital) * 100;
      } else {
        cumulativeAbsoluteProfit += absoluteProfit;
        currentYValue = cumulativeAbsoluteProfit;
      }

      spots.add(FlSpot(timeStamp, lastYValue));
      spots.add(FlSpot(timeStamp, currentYValue));
      lastYValue = currentYValue;
    }

    final lastTradeDate = DateTime.parse(sortedTrades.last['close_date']);
    return (spots: spots, firstDate: firstTradeDate, lastDate: lastTradeDate, isPercentageChart: isPercentageChart);
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<dynamic>>(
      key: ValueKey(apiService.baseUrl),
      future: Future.wait([
        apiService.getProfitSummary(),
        apiService.getClosedTrades(limit: 500),
        apiService.showConfig(),
        apiService.getBalance(),
      ]),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.waiting) {
          return const Center(child: CircularProgressIndicator());
        }
        if (snapshot.hasError) {
          return Center(child: Text('Error: ${snapshot.error}'));
        }
        if (!snapshot.hasData) {
          return const Center(child: Text('Could not load dashboard data.'));
        }

        final profitSummary = snapshot.data![0] as Map<String, dynamic>;
        final trades = snapshot.data![1] as List<dynamic>;
        final config = snapshot.data![2] as Map<String, dynamic>;
        final balanceData = snapshot.data![3] as Map<String, dynamic>;

        // Data from profitSummary
        final overallProfitPercent = profitSummary['profit_closed_percent']?.toDouble() ?? 0.0;
        // stakeCurrency will be defined below from the /balance endpoint
        final tradeCount = profitSummary['closed_trade_count'] ?? 0;
        final avgProfit = profitSummary['profit_closed_percent_mean']?.toDouble() ?? 0.0;
        final avgDuration = profitSummary['avg_duration'] ?? 'N/A';
        final bestPair = profitSummary['best_pair'] ?? 'N/A';
        final tradingVolume = profitSummary['trading_volume']?.toDouble() ?? 0.0;


        // --- NEW LOGIC FOR STAKE CURRENCY AND FREE BALANCE ---
        String stakeCurrency = profitSummary['stake_currency'] ?? ''; // Get from /profit first
        double freeBalance = 0.0;

        try {
          if (balanceData['currencies'] is List) {
            final currencies = balanceData['currencies'] as List<dynamic>;
            
            // Find the stake currency data (the one that is not a position)
            final stakeCurrencyData = currencies.firstWhere(
              (c) => c['is_position'] == false,
              orElse: () => null,
            );

            if (stakeCurrencyData != null) {
              freeBalance = stakeCurrencyData['free']?.toDouble() ?? 0.0;
              // If stakeCurrency was empty from /profit, get it from /balance
              if (stakeCurrency.isEmpty) {
                stakeCurrency = stakeCurrencyData['currency'] ?? '';
              }
            }
          }
        } catch (e) {
          print('Error parsing free balance from /balance: $e');
        }
        // --- END OF NEW LOGIC ---


        // Data from config
        final strategy = config['strategy'] ?? 'N/A';
        final exchange = config['exchange'] ?? 'N/A';
        final stoplossOnExchange = config['stoploss_on_exchange'] ?? false;
        final tradingMode = config['trading_mode']?.toString().toUpperCase() ?? 'N/A';
        final stoploss = config['stoploss']?.toDouble() ?? 0.0;
        final timeframe = config['timeframe'] ?? 'N/A';
        final maxOpenTrades = config['max_open_trades']?.toString() ?? 'N/A';
        final stakeAmount = config['stake_amount']?.toString() ?? 'N/A';
        final shortAllowed = config['short_allowed'] ?? false;

        final startingCapital = profitSummary['starting_capital']?.toDouble() ?? 0.0;

        if (trades.isEmpty) {
          return const Center(child: Text('No trades found to build dashboard.'));
        }

        final chartData = _prepareChartData(trades, startingCapital);
        final spots = chartData.spots;
        final isPercentageChart = chartData.isPercentageChart;

        double bottomTitleInterval;
        if (chartData.firstDate != null && chartData.lastDate != null) {
          final differenceInMs = chartData.lastDate!.difference(chartData.firstDate!).inMilliseconds;
          if (differenceInMs > 0) {
            bottomTitleInterval = differenceInMs / 4;
          } else {
            bottomTitleInterval = const Duration(days: 1).inMilliseconds.toDouble();
          }
        } else {
          bottomTitleInterval = const Duration(days: 1).inMilliseconds.toDouble();
        }
        try {
          // Check if 'currencies' key exists and is a List
          if (balanceData['currencies'] is List) {
            final currencies = balanceData['currencies'] as List<dynamic>;
            
            // Find the data for our stake_currency (e.g., 'USDT')
            final stakeCurrencyData = currencies.firstWhere(
              (c) => c['currency'] == stakeCurrency,
              orElse: () => null, // Return null if not found
            );

            if (stakeCurrencyData != null) {
              // Get the 'free' value from that currency's data
              freeBalance = stakeCurrencyData['free']?.toDouble() ?? 0.0;
            }
          }
        } catch (e) {
          print('Error parsing free balance: $e'); // For debugging
        }

        return AnimatedContainer(
          duration: const Duration(milliseconds: 500),
          curve: Curves.easeInOut,
          color: overallProfitPercent > 0 ? Colors.green.withOpacity(0.1) : (overallProfitPercent < 0 ? Colors.red.withOpacity(0.1) : Colors.transparent),
          child: SingleChildScrollView(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // --- THIS IS THE CHART SECTION THAT WAS ACCIDENTALLY REMOVED ---
                  Text('Cumulative Profit (${trades.length} trades)', style: Theme.of(context).textTheme.titleLarge),
                  Text('Overall Profit: ${overallProfitPercent.toStringAsFixed(2)}%', style: Theme.of(context).textTheme.bodyMedium),
                  const SizedBox(height: 24),
                  SizedBox(
                    height: 300,
                    child: LineChart(
                      LineChartData(
                        lineTouchData: LineTouchData(
                            touchTooltipData: LineTouchTooltipData(
                                getTooltipItems: (touchedSpots) {
                                  return touchedSpots.map((spot) {
                                    final date = DateTime.fromMillisecondsSinceEpoch(spot.x.toInt());
                                    final dateString = DateFormat('d MMM yyyy').format(date);
                                    final valueString = isPercentageChart
                                        ? '${spot.y.toStringAsFixed(2)}%'
                                        : '${spot.y.toStringAsFixed(2)} $stakeCurrency';

                                    return LineTooltipItem(
                                        '$valueString\n',
                                        const TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
                                        children: [TextSpan(text: dateString, style: const TextStyle(fontWeight: FontWeight.normal))]
                                    );
                                  }).toList();
                                }
                            )
                        ),
                        titlesData: FlTitlesData(
                          leftTitles: AxisTitles(
                            sideTitles: SideTitles(
                              showTitles: true,
                              reservedSize: 50,
                              getTitlesWidget: (value, meta) {
                                if (value == meta.max || value == meta.min) return Container();
                                String text = isPercentageChart ? '${value.toStringAsFixed(0)}%' : value.toStringAsFixed(0);
                                return Padding(
                                  padding: const EdgeInsets.only(right: 8.0),
                                  child: Text(text, textAlign: TextAlign.right, style: const TextStyle(fontSize: 10)),
                                );
                              },
                            ),
                          ),
                          bottomTitles: AxisTitles(
                            sideTitles: SideTitles(
                              showTitles: true,
                              reservedSize: 30,
                              interval: bottomTitleInterval,
                              getTitlesWidget: (value, meta) {
                                if (value == meta.max || value == meta.min) return Container();
                                final date = DateTime.fromMillisecondsSinceEpoch(value.toInt());
                                return SideTitleWidget(
                                  axisSide: meta.axisSide,
                                  space: 8.0,
                                  child: Text(DateFormat('d/M').format(date), style: const TextStyle(fontSize: 10)),
                                );
                              },
                            ),
                          ),
                          topTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
                          rightTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
                        ),
                        gridData: FlGridData(show: true, drawVerticalLine: true),
                        borderData: FlBorderData(show: true),
                        minX: chartData.firstDate?.millisecondsSinceEpoch.toDouble(),
                        maxX: chartData.lastDate?.millisecondsSinceEpoch.toDouble(),
                        lineBarsData: [
                          LineChartBarData(
                            spots: spots,
                            isCurved: false,
                            isStrokeCapRound: false,
                            color: Colors.blueAccent,
                            barWidth: 3,
                            dotData: const FlDotData(show: false),
                            belowBarData: BarAreaData(show: true, gradient: LinearGradient(colors: [Colors.blueAccent.withOpacity(0.3), Colors.blueAccent.withOpacity(0.0)], begin: Alignment.topCenter, end: Alignment.bottomCenter)),
                          ),
                        ],
                      ),
                    ),
                  ),
                  // --- END OF RESTORED CHART SECTION ---

                  const Divider(height: 32, thickness: 0.5),
                  Text('Performance Stats', style: Theme.of(context).textTheme.titleLarge),
                  const SizedBox(height: 16),
                  LayoutBuilder(
                    builder: (context, constraints) {
                      final double tileWidth = (constraints.maxWidth - 10) / 2;
                      return Wrap(
                        spacing: 10,
                        runSpacing: 10,
                        children: [
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.analytics_outlined, title: 'Avg Profit / Trade', value: '${avgProfit.toStringAsFixed(2)}%', valueColor: avgProfit >= 0 ? Colors.green : Colors.red)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.functions, title: 'Total Closed Trades', value: '$tradeCount')),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.access_time, title: 'Avg Duration', value: avgDuration)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.star_border, title: 'Best Pair', value: bestPair)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.bar_chart, title: 'Trading Volume', value: '${tradingVolume.toStringAsFixed(0)} $stakeCurrency')),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.account_balance_wallet_outlined, title: 'Free Balance', value: '${freeBalance.toStringAsFixed(2)} $stakeCurrency')),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.memory, title: 'Strategy', value: strategy)),
                        ],
                      );
                    },
                  ),

                  const Divider(height: 32, thickness: 0.5),
                  Text('Configuration', style: Theme.of(context).textTheme.titleLarge),
                  const SizedBox(height: 16),
                  LayoutBuilder(
                    builder: (context, constraints) {
                      final double tileWidth = (constraints.maxWidth - 10) / 2;
                      return Wrap(
                        spacing: 10,
                        runSpacing: 10,
                        children: [
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.store_mall_directory_outlined, title: 'Exchange', value: exchange.toString().toUpperCase())),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.shield_outlined, title: 'Stoploss On Exchange', value: stoplossOnExchange ? 'Enabled' : 'Disabled', valueColor: stoplossOnExchange ? Colors.green : Colors.orange)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.trending_up, title: 'Trading Mode', value: tradingMode)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.timer_outlined, title: 'Timeframe', value: timeframe)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.attach_money, title: 'Stake Amount', value: stakeAmount.toString().toUpperCase())),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.playlist_add_check, title: 'Max Open Trades', value: maxOpenTrades)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.arrow_downward, title: 'Stoploss', value: stoploss == -1.0 ? 'Disabled' : '${(stoploss * 100).toStringAsFixed(2)}%', valueColor: stoploss == -1.0 ? Colors.grey : Colors.orange)),
                          SizedBox(width: tileWidth, child: StatTile(icon: Icons.switch_left, title: 'Shorting Allowed', value: shortAllowed ? 'Yes' : 'No', valueColor: shortAllowed ? Colors.green : Colors.grey)),
                        ],
                      );
                    },
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
