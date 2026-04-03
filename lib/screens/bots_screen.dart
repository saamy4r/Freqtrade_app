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
          statusColor = Colors.grey;
        } else if (isOnline == true) {
          statusColor = Colors.green;
        } else {
          statusColor = Colors.red;
        }

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
