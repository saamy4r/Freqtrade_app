import 'package:flutter/material.dart';
import '../services/api_service.dart';
import '../widgets/error_view.dart';

class LogsScreen extends StatefulWidget {
  final ApiService apiService;
  const LogsScreen({super.key, required this.apiService});

  @override
  State<LogsScreen> createState() => _LogsScreenState();
}

class _LogsScreenState extends State<LogsScreen> with AutomaticKeepAliveClientMixin {
  List<dynamic>? _logs;
  String? _error;
  final ScrollController _scrollController = ScrollController();

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _fetchLogs();
  }

  @override
  void didUpdateWidget(covariant LogsScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.apiService.baseUrl != oldWidget.apiService.baseUrl) {
      _fetchLogs();
    }
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _fetchLogs() async {
    setState(() {
      _logs = null;
      _error = null;
    });
    try {
      final logs = await widget.apiService.getLogs(limit: 500);
      if (!mounted) return;
      setState(() => _logs = logs);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_scrollController.hasClients) {
          _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  Color _levelColor(String level) {
    switch (level.toUpperCase()) {
      case 'ERROR':
      case 'CRITICAL':
        return Colors.red.shade400;
      case 'WARNING':
        return Colors.amber.shade400;
      case 'DEBUG':
        return Colors.grey.shade500;
      default:
        return Colors.blue.shade300;
    }
  }

  Color _levelBgColor(String level) {
    switch (level.toUpperCase()) {
      case 'ERROR':
      case 'CRITICAL':
        return Colors.red.withOpacity(0.15);
      case 'WARNING':
        return Colors.amber.withOpacity(0.12);
      default:
        return Colors.transparent;
    }
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);

    if (_error != null) {
      return ErrorView(message: _error!, onRetry: _fetchLogs);
    }

    if (_logs == null) {
      return const Center(child: CircularProgressIndicator());
    }

    if (_logs!.isEmpty) {
      return const Center(child: Text('No logs available.'));
    }

    return Column(
      children: [
        Container(
          color: const Color(0xFF1E1E1E),
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
          child: Row(
            children: [
              const Icon(Icons.terminal, color: Colors.green, size: 18),
              const SizedBox(width: 8),
              Text(
                '${_logs!.length} log entries',
                style: const TextStyle(
                  color: Colors.green,
                  fontFamily: 'monospace',
                  fontSize: 13,
                  fontWeight: FontWeight.bold,
                ),
              ),
              const Spacer(),
              IconButton(
                icon: const Icon(Icons.refresh, color: Colors.green, size: 20),
                onPressed: _fetchLogs,
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                tooltip: 'Refresh logs',
              ),
              const SizedBox(width: 8),
              IconButton(
                icon: const Icon(Icons.arrow_downward, color: Colors.green, size: 20),
                onPressed: () {
                  if (_scrollController.hasClients) {
                    _scrollController.animateTo(
                      _scrollController.position.maxScrollExtent,
                      duration: const Duration(milliseconds: 300),
                      curve: Curves.easeOut,
                    );
                  }
                },
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                tooltip: 'Scroll to bottom',
              ),
            ],
          ),
        ),
        Expanded(
          child: RefreshIndicator(
            onRefresh: _fetchLogs,
            child: Container(
              color: const Color(0xFF1E1E1E),
              child: ListView.builder(
                controller: _scrollController,
                padding: const EdgeInsets.only(bottom: 16),
                itemCount: _logs!.length,
                itemBuilder: (context, index) {
                  final entry = _logs![index];
                  if (entry is! List || entry.length < 5) return const SizedBox.shrink();

                  final timeStr = entry[0].toString();
                  final timePart = timeStr.length >= 19 ? timeStr.substring(11, 19) : timeStr;
                  final level = entry[3].toString();
                  final message = entry[4].toString();

                  final levelColor = _levelColor(level);
                  final bgColor = _levelBgColor(level);

                  return Container(
                    color: bgColor,
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          timePart,
                          style: const TextStyle(
                            color: Color(0xFF888888),
                            fontSize: 11,
                            fontFamily: 'monospace',
                          ),
                        ),
                        const SizedBox(width: 8),
                        Container(
                          width: 58,
                          alignment: Alignment.center,
                          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                          decoration: BoxDecoration(
                            color: levelColor.withOpacity(0.15),
                            borderRadius: BorderRadius.circular(4),
                            border: Border.all(color: levelColor.withOpacity(0.5), width: 0.5),
                          ),
                          child: Text(
                            level.toUpperCase(),
                            style: TextStyle(
                              color: levelColor,
                              fontSize: 10,
                              fontWeight: FontWeight.bold,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            message,
                            style: TextStyle(
                              color: level.toUpperCase() == 'DEBUG'
                                  ? const Color(0xFF888888)
                                  : const Color(0xFFD4D4D4),
                              fontSize: 12,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ),
                      ],
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      ],
    );
  }
}
