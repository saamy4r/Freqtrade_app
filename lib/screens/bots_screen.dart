import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:uuid/uuid.dart';
import '../models/bot.dart';
import '../services/api_service.dart';

class BotsScreen extends StatefulWidget {
  final List<Bot> bots;
  final Bot? activeBot;
  final VoidCallback onAddBot;
  final Function(Bot) onSelectBot;
  final Function(String) onDeleteBot;
  final Function(int oldIndex, int newIndex) onReorderBots;

  const BotsScreen({
    super.key,
    required this.bots,
    required this.activeBot,
    required this.onAddBot,
    required this.onSelectBot,
    required this.onDeleteBot,
    required this.onReorderBots,
  });

  @override
  State<BotsScreen> createState() => _BotsScreenState();
}

class _BotsScreenState extends State<BotsScreen> {
  Map<String, bool> _onlineStatus = {};
  bool _isLoadingStatus = true;

  @override
  void initState() {
    super.initState();
    _checkAllBotStatus();
  }

  Future<void> _checkAllBotStatus() async {
    if (!mounted) return;
    setState(() {
      _isLoadingStatus = true;
    });

    final pings = widget.bots.map((bot) => ApiService.ping(bot.url)).toList();
    final results = await Future.wait(pings);
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

  Future<void> _confirmDelete(Bot bot) async {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: isDark ? const Color(0xFF3A1B1B) : Colors.red.shade50,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(20),
          side: const BorderSide(color: Colors.red, width: 1.5),
        ),
        titlePadding: const EdgeInsets.fromLTRB(20, 20, 20, 0),
        title: Row(
          children: [
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: Colors.red.withValues(alpha: 0.2),
                borderRadius: BorderRadius.circular(12),
              ),
              child: const Icon(Icons.smart_toy_outlined, color: Colors.red, size: 22),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('Remove Bot', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 18)),
                  Text(
                    bot.name,
                    style: TextStyle(fontSize: 13, fontWeight: FontWeight.w500, color: Colors.red.shade700),
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
          ],
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Divider(height: 20),
            Text(
              'Remove "${bot.name}" from your bot list?\n\nThis also deletes all locally cached data for this bot.',
              style: const TextStyle(fontSize: 13),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton.icon(
            onPressed: () => Navigator.of(ctx).pop(true),
            icon: const Icon(Icons.delete_forever, size: 18),
            label: const Text('Remove'),
            style: FilledButton.styleFrom(backgroundColor: Colors.red.shade600),
          ),
        ],
      ),
    );
    if (confirmed == true && mounted) widget.onDeleteBot(bot.id);
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

    return ReorderableListView(
      onReorder: (oldIndex, newIndex) {
        if (newIndex > oldIndex) newIndex--;
        widget.onReorderBots(oldIndex, newIndex);
      },
      footer: Padding(
        padding: const EdgeInsets.all(16.0),
        child: OutlinedButton.icon(
          icon: const Icon(Icons.add),
          label: const Text("Add Another Bot"),
          onPressed: widget.onAddBot,
        ),
      ),
      children: widget.bots.asMap().entries.map((entry) {
        final index = entry.key;
        final bot = entry.value;
        final bool isActive = bot.id == widget.activeBot?.id;
        final bool? isOnline = _onlineStatus[bot.id];

        Color statusColor;
        if (_isLoadingStatus) {
          statusColor = Colors.grey;
        } else if (isOnline == true) {
          statusColor = Colors.green;
        } else {
          statusColor = Colors.red;
        }

        return Card(
          key: ValueKey(bot.id),
          margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          child: ListTile(
            leading: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                ReorderableDragStartListener(
                  index: index,
                  child: const Icon(Icons.drag_handle, color: Colors.grey, size: 20),
                ),
                const SizedBox(width: 8),
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(color: statusColor, shape: BoxShape.circle),
                ),
                const SizedBox(width: 10),
                Icon(Icons.smart_toy_outlined, color: isActive ? Theme.of(context).primaryColor : Colors.grey),
              ],
            ),
            title: Text(bot.name, style: TextStyle(fontWeight: isActive ? FontWeight.bold : FontWeight.normal)),
            subtitle: Text(bot.url, overflow: TextOverflow.ellipsis),
            trailing: IconButton(
              icon: const Icon(Icons.delete_outline, color: Colors.red),
              onPressed: () => _confirmDelete(bot),
            ),
            onTap: () => widget.onSelectBot(bot),
            tileColor: isActive ? Theme.of(context).primaryColor.withValues(alpha: 0.1) : null,
          ),
        );
      }).toList(),
    );
  }
}

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
        _errorMessage = 'All fields are required.';
      });
      return;
    }

    setState(() {
      _isLoading = true;
      _errorMessage = null;
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
    } on TimeoutException {
      if (mounted) {
        setState(() {
          _errorMessage = 'Connection timed out. Check the URL or port.';
        });
      }
    } on SocketException {
      if (mounted) {
        setState(() {
          _errorMessage = 'Connection failed. Check the URL or port.';
        });
      }
    } on Exception catch (e) {
      if (mounted) {
        setState(() {
          if (e.toString().contains('Login failed')) {
            _errorMessage = 'Login failed. Check username or password.';
          } else {
            _errorMessage = 'An unknown error occurred.';
          }
        });
      }
    } finally {
      if (mounted) {
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
