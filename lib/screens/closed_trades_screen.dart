import 'package:flutter/material.dart';
import '../services/api_service.dart';
import '../widgets/error_view.dart';

class ClosedTradesScreen extends StatefulWidget {
  final ApiService apiService;
  const ClosedTradesScreen({super.key, required this.apiService});

  @override
  State<ClosedTradesScreen> createState() => _ClosedTradesScreenState();
}

class _ClosedTradesScreenState extends State<ClosedTradesScreen> with AutomaticKeepAliveClientMixin {
  Map<String, dynamic>? _profitSummary;
  List<dynamic>? _trades;
  Map<String, dynamic>? _balanceData;
  String? _error;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _fetchData();
  }

  @override
  void didUpdateWidget(covariant ClosedTradesScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.apiService.baseUrl != oldWidget.apiService.baseUrl) {
      _fetchData();
    }
  }

  Future<void> _fetchData() async {
    setState(() {
      _profitSummary = null;
      _trades = null;
      _balanceData = null;
      _error = null;
    });
    try {
      final results = await Future.wait([
        widget.apiService.getProfitSummary(),
        widget.apiService.getClosedTrades(limit: 500),
        widget.apiService.getBalance(),
      ]);
      if (!mounted) return;
      final trades = results[1] as List<dynamic>;
      trades.sort((a, b) {
        final dateA = a['close_date'] ?? '';
        final dateB = b['close_date'] ?? '';
        return dateB.compareTo(dateA);
      });
      setState(() {
        _profitSummary = results[0] as Map<String, dynamic>;
        _trades = trades;
        _balanceData = results[2] as Map<String, dynamic>;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
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
      return ErrorView(message: _error!, onRetry: _fetchData);
    }

    if (_profitSummary == null || _trades == null || _balanceData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final totalProfit = _profitSummary!['profit_closed_coin']?.toDouble() ?? 0.0;
    final currency = _profitSummary!['stake_currency'] ?? '';
    final totalPortfolioValue = _balanceData!['total']?.toDouble() ?? 0.0;

    if (_trades!.isEmpty) {
      return const Center(child: Text('No closed trades found.'));
    }

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
            title: const Text('Current Portfolio Value', style: TextStyle(fontWeight: FontWeight.bold)),
            trailing: Text(
              '${totalPortfolioValue.toStringAsFixed(2)} $currency',
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
          child: RefreshIndicator(
            onRefresh: _fetchData,
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(horizontal: 8.0),
              itemCount: _trades!.length,
              itemBuilder: (context, index) {
                final trade = _trades![index];
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
        ),
      ],
    );
  }
}
