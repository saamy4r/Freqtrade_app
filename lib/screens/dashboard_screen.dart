import 'package:flutter/material.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:intl/intl.dart';
import '../services/api_service.dart';
import '../widgets/stat_tile.dart';
import '../widgets/error_view.dart';

class DashboardScreen extends StatefulWidget {
  final ApiService apiService;
  const DashboardScreen({super.key, required this.apiService});

  @override
  State<DashboardScreen> createState() => _DashboardScreenState();
}

class _DashboardScreenState extends State<DashboardScreen> with AutomaticKeepAliveClientMixin {
  Map<String, dynamic>? _profitSummary;
  List<dynamic>? _trades;
  Map<String, dynamic>? _config;
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
  void didUpdateWidget(covariant DashboardScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.apiService.baseUrl != oldWidget.apiService.baseUrl) {
      _fetchData();
    }
  }

  Future<void> _fetchData() async {
    setState(() {
      _profitSummary = null;
      _trades = null;
      _config = null;
      _balanceData = null;
      _error = null;
    });
    try {
      final results = await Future.wait([
        widget.apiService.getProfitSummary(),
        widget.apiService.getClosedTrades(limit: 500),
        widget.apiService.showConfig(),
        widget.apiService.getBalance(),
      ]);
      if (!mounted) return;
      setState(() {
        _profitSummary = results[0] as Map<String, dynamic>;
        _trades = results[1] as List<dynamic>;
        _config = results[2] as Map<String, dynamic>;
        _balanceData = results[3] as Map<String, dynamic>;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

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
    super.build(context);

    if (_error != null) {
      return ErrorView(message: _error!, onRetry: _fetchData);
    }

    if (_profitSummary == null || _trades == null || _config == null || _balanceData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final profitSummary = _profitSummary!;
    final trades = _trades!;
    final config = _config!;
    final balanceData = _balanceData!;

    final overallProfitPercent = profitSummary['profit_closed_percent']?.toDouble() ?? 0.0;
    final tradeCount = profitSummary['closed_trade_count'] ?? 0;
    final avgProfit = profitSummary['profit_closed_percent_mean']?.toDouble() ?? 0.0;
    final avgDuration = profitSummary['avg_duration'] ?? 'N/A';
    final bestPair = profitSummary['best_pair'] ?? 'N/A';
    final tradingVolume = profitSummary['trading_volume']?.toDouble() ?? 0.0;

    String stakeCurrency = profitSummary['stake_currency'] ?? '';
    double freeBalance = 0.0;

    if (balanceData['currencies'] is List) {
      final currencies = balanceData['currencies'] as List<dynamic>;
      final stakeCurrencyData = currencies.firstWhere(
        (c) => c['is_position'] == false,
        orElse: () => null,
      );
      if (stakeCurrencyData != null) {
        freeBalance = stakeCurrencyData['free']?.toDouble() ?? 0.0;
        if (stakeCurrency.isEmpty) {
          stakeCurrency = stakeCurrencyData['currency'] ?? '';
        }
      }
    }

    // Refine freeBalance using the matched stake currency entry
    if (balanceData['currencies'] is List && stakeCurrency.isNotEmpty) {
      final currencies = balanceData['currencies'] as List<dynamic>;
      final stakeCurrencyData = currencies.firstWhere(
        (c) => c['currency'] == stakeCurrency,
        orElse: () => null,
      );
      if (stakeCurrencyData != null) {
        freeBalance = stakeCurrencyData['free']?.toDouble() ?? freeBalance;
      }
    }

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

    return RefreshIndicator(
      onRefresh: _fetchData,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 500),
        curve: Curves.easeInOut,
        color: overallProfitPercent > 0
            ? Colors.green.withOpacity(0.1)
            : (overallProfitPercent < 0 ? Colors.red.withOpacity(0.1) : Colors.transparent),
        child: SingleChildScrollView(
          physics: const AlwaysScrollableScrollPhysics(),
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
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
                                children: [TextSpan(text: dateString, style: const TextStyle(fontWeight: FontWeight.normal))],
                              );
                            }).toList();
                          },
                        ),
                      ),
                      titlesData: FlTitlesData(
                        leftTitles: AxisTitles(
                          sideTitles: SideTitles(
                            showTitles: true,
                            reservedSize: 50,
                            getTitlesWidget: (value, meta) {
                              if (value == meta.max || value == meta.min) return Container();
                              final text = isPercentageChart ? '${value.toStringAsFixed(0)}%' : value.toStringAsFixed(0);
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
                      gridData: const FlGridData(show: true, drawVerticalLine: true),
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
                          belowBarData: BarAreaData(
                            show: true,
                            gradient: LinearGradient(
                              colors: [Colors.blueAccent.withOpacity(0.3), Colors.blueAccent.withOpacity(0.0)],
                              begin: Alignment.topCenter,
                              end: Alignment.bottomCenter,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),

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
      ),
    );
  }
}
