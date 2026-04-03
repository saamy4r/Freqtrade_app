import 'package:flutter/material.dart';
import '../services/api_service.dart';
import '../widgets/error_view.dart';

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
      final tradesResult = results[0] as List<dynamic>;
      final balanceResult = results[1] as Map<String, dynamic>;
      double freeBalance = 0.0;
      double stakedAmount = 0.0;
      if (balanceResult['currencies'] is List) {
        final currencies = balanceResult['currencies'] as List<dynamic>;
        final stakeCurrencyData = currencies.firstWhere(
          (c) => c['is_position'] == false,
          orElse: () => null,
        );
        if (stakeCurrencyData != null) {
          freeBalance = stakeCurrencyData['free']?.toDouble() ?? 0.0;
          stakedAmount = stakeCurrencyData['used']?.toDouble() ?? 0.0;
        }
      }
      tradesResult.sort((a, b) {
        final dateA = a['open_date'] ?? '';
        final dateB = b['open_date'] ?? '';
        return dateB.compareTo(dateA);
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

  Future<void> _showExitDialog(Map<String, dynamic> trade) async {
    final tradeId = trade['trade_id']?.toString() ?? '';
    final pair = trade['pair'] ?? 'Unknown';
    final currentRate = (trade['current_rate'] ?? 0.0).toDouble();
    final profitRatio = (trade['profit_ratio'] ?? 0.0).toDouble();
    final profitAbs = (trade['profit_abs'] ?? 0.0).toDouble();
    final currency = _balance?['stake_currency'] ?? '';

    await showDialog(
      context: context,
      builder: (ctx) => _ExitTradeDialog(
        pair: pair,
        tradeId: tradeId,
        currentRate: currentRate,
        profitRatio: profitRatio,
        profitAbs: profitAbs,
        currency: currency,
        apiService: widget.apiService,
        onSuccess: () {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text('Exit order placed for $pair'),
              backgroundColor: Colors.green,
            ),
          );
          _fetchData();
        },
      ),
    );
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
    if (_trades == null || _balance == null || _freeBalance == null || _stakedAmount == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final totalOpenProfit = _trades!.fold<double>(
      0.0,
      (sum, trade) => sum + (trade['profit_abs'] ?? 0.0),
    );
    final currency = _balance!['stake_currency'] ?? '';
    final cashBalance = _balance!['total']?.toDouble() ?? 0.0;
    final double freeBalance = _freeBalance!;
    final double stakedAmount = _stakedAmount!;
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
          margin: const EdgeInsets.fromLTRB(12, 12, 12, 0),
          elevation: 0,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
            side: BorderSide(color: Colors.grey.withOpacity(0.3), width: 1),
          ),
          child: ListTile(
            leading: const Icon(
              Icons.attach_money,
              color: Colors.green,
            ),
            title: const Text('Free / Staked', style: TextStyle(fontWeight: FontWeight.bold)),
            trailing: Text(
              '${freeBalance.toStringAsFixed(2)} / ${stakedAmount.toStringAsFixed(2)} $currency',
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
                                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                children: [
                                  Text('Opened: $openDate', style: const TextStyle(fontSize: 11, color: Colors.grey)),
                                  TextButton.icon(
                                    onPressed: () => _showExitDialog(trade),
                                    icon: const Icon(Icons.exit_to_app, size: 16),
                                    label: const Text('Exit', style: TextStyle(fontSize: 12)),
                                    style: TextButton.styleFrom(
                                      foregroundColor: Colors.orange,
                                      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                                      visualDensity: VisualDensity.compact,
                                    ),
                                  ),
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

class _ExitTradeDialog extends StatefulWidget {
  final String pair;
  final String tradeId;
  final double currentRate;
  final double profitRatio;
  final double profitAbs;
  final String currency;
  final ApiService apiService;
  final VoidCallback onSuccess;

  const _ExitTradeDialog({
    required this.pair,
    required this.tradeId,
    required this.currentRate,
    required this.profitRatio,
    required this.profitAbs,
    required this.currency,
    required this.apiService,
    required this.onSuccess,
  });

  @override
  State<_ExitTradeDialog> createState() => _ExitTradeDialogState();
}

class _ExitTradeDialogState extends State<_ExitTradeDialog> {
  bool _isLoading = false;
  String? _error;

  Future<void> _submitExit(String orderType) async {
    setState(() {
      _isLoading = true;
      _error = null;
    });
    try {
      await widget.apiService.forceExit(widget.tradeId, orderType);
      if (mounted) {
        Navigator.of(context).pop();
        widget.onSuccess();
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _isLoading = false;
          _error = e.toString().replaceFirst('Exception: ', '');
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final bgColor = isDark ? const Color(0xFF1B3A2D) : Colors.green.shade50;
    final profitColor = widget.profitRatio >= 0 ? Colors.green.shade700 : Colors.red.shade600;
    final profitSign = widget.profitRatio >= 0 ? '+' : '';

    return AlertDialog(
      backgroundColor: bgColor,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(20),
        side: const BorderSide(color: Colors.black, width: 1.5),
      ),
      titlePadding: const EdgeInsets.fromLTRB(20, 20, 20, 0),
      title: Row(
        children: [
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: Colors.green.withOpacity(0.2),
              borderRadius: BorderRadius.circular(12),
            ),
            child: const Icon(Icons.exit_to_app, color: Colors.green, size: 22),
          ),
          const SizedBox(width: 12),
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Force Exit',
                style: TextStyle(fontWeight: FontWeight.bold, fontSize: 18),
              ),
              Text(
                widget.pair,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: Colors.green.shade700,
                ),
              ),
            ],
          ),
        ],
      ),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Divider(height: 20),
          Row(
            children: [
              const Icon(Icons.speed, size: 16, color: Colors.grey),
              const SizedBox(width: 6),
              const Text('Current Price', style: TextStyle(color: Colors.grey, fontSize: 13)),
              const Spacer(),
              Text(
                widget.currentRate.toStringAsFixed(6),
                style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(
              color: profitColor.withOpacity(0.1),
              borderRadius: BorderRadius.circular(10),
              border: Border.all(color: profitColor.withOpacity(0.35)),
            ),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Row(
                  children: [
                    Icon(
                      widget.profitRatio >= 0 ? Icons.trending_up : Icons.trending_down,
                      size: 16,
                      color: profitColor,
                    ),
                    const SizedBox(width: 6),
                    Text('Current P/L', style: TextStyle(color: profitColor, fontSize: 13, fontWeight: FontWeight.w500)),
                  ],
                ),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Text(
                      '$profitSign${(widget.profitRatio * 100).toStringAsFixed(2)}%',
                      style: TextStyle(color: profitColor, fontWeight: FontWeight.bold, fontSize: 15),
                    ),
                    Text(
                      '$profitSign${widget.profitAbs.toStringAsFixed(2)} ${widget.currency}',
                      style: TextStyle(color: profitColor, fontSize: 12),
                    ),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 10),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Icon(Icons.info_outline, size: 14, color: Colors.grey),
              const SizedBox(width: 6),
              const Expanded(
                child: Text(
                  'A limit order will be placed at the current bid price.',
                  style: TextStyle(fontSize: 12, color: Colors.grey),
                ),
              ),
            ],
          ),
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(
              _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error, fontSize: 13),
            ),
          ],
        ],
      ),
      actions: _isLoading
          ? [const Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator())]
          : [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
              FilledButton.icon(
                onPressed: () => _submitExit('limit'),
                icon: const Icon(Icons.price_check, size: 18),
                label: const Text('Limit Exit'),
                style: FilledButton.styleFrom(backgroundColor: Colors.green.shade600),
              ),
            ],
    );
  }
}
