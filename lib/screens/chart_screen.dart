import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:intl/intl.dart';
import '../services/api_service.dart';
import '../widgets/error_view.dart';

class ChartScreen extends StatefulWidget {
  final ApiService apiService;
  const ChartScreen({super.key, required this.apiService});

  @override
  State<ChartScreen> createState() => _ChartScreenState();
}

class _ChartScreenState extends State<ChartScreen> with AutomaticKeepAliveClientMixin {
  List<dynamic> _openTrades = [];
  List<dynamic> _closedTrades = [];
  List<String> _whitelist = [];
  String? _selectedPair;
  String _timeframe = '5m';
  Map<String, dynamic>? _candleData;
  bool _isLoading = true;
  bool _isLoadingChart = false;
  String? _error;
  String? _chartError;

  List<FlSpot> _allSpots = [];
  int _viewOffset = 0;
  static const int _viewCount = 100;
  double? _dragAnchorX;
  int? _dragAnchorOffset;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    _fetchInitialData();
  }

  @override
  void didUpdateWidget(covariant ChartScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.apiService.baseUrl != oldWidget.apiService.baseUrl) {
      _fetchInitialData();
    }
  }

  Future<void> _fetchInitialData() async {
    // Bug 1 fix: reset _selectedPair so bot switch re-selects correct pair
    setState(() {
      _isLoading = true;
      _error = null;
      _selectedPair = null;
    });
    try {
      final results = await Future.wait([
        widget.apiService.getOpenTrades(),
        widget.apiService.getClosedTrades(limit: 200),
        widget.apiService.showConfig(),
        widget.apiService.getWhitelist(),
      ]);
      if (!mounted) return;
      final open = results[0] as List<dynamic>;
      final closed = results[1] as List<dynamic>;
      final config = results[2] as Map<String, dynamic>;
      final whitelist = results[3] as List<String>;

      final configTf = config['timeframe']?.toString() ?? '5m';
      setState(() {
        _openTrades = open;
        _closedTrades = closed;
        _whitelist = whitelist;
        _timeframe = configTf;
        _isLoading = false;
      });

      // Auto-select first open trade pair, otherwise first whitelist pair
      final firstPair = open.isNotEmpty
          ? open.first['pair'] as String
          : whitelist.isNotEmpty ? whitelist.first : null;
      if (firstPair != null) await _loadChart(firstPair);
    } catch (e) {
      if (!mounted) return;
      setState(() { _isLoading = false; _error = e.toString(); });
    }
  }

  Future<void> _loadChart(String pair) async {
    setState(() {
      _selectedPair = pair;
      _isLoadingChart = true;
      _chartError = null;
      _candleData = null;
    });
    try {
      final data = await widget.apiService.getPairCandles(pair, _timeframe, limit: 300);
      if (!mounted) return;
      setState(() {
        _candleData = data;
        _isLoadingChart = false;
        _viewOffset = 0;
      });
      _computeSpots();
    } catch (e) {
      if (!mounted) return;
      setState(() { _isLoadingChart = false; _chartError = e.toString(); });
    }
  }

  void _computeSpots() {
    if (_candleData == null) { setState(() => _allSpots = []); return; }
    final columns = List<String>.from(_candleData!['columns'] ?? []);
    final rawData = List<dynamic>.from(_candleData!['data'] ?? []);
    final dateIdx = columns.indexOf('date');
    final closeIdx = columns.indexOf('close');
    if (dateIdx < 0 || closeIdx < 0) { setState(() => _allSpots = []); return; }

    final spots = <FlSpot>[];
    for (final c in rawData) {
      final rawDate = c[dateIdx];
      final double ts;
      if (rawDate is num) {
        ts = rawDate.toDouble();
      } else {
        var s = rawDate.toString().trim().replaceFirst(' ', 'T');
        if (!s.contains('+') && !s.endsWith('Z')) s += 'Z';
        final dt = DateTime.tryParse(s);
        if (dt == null) continue;
        ts = dt.millisecondsSinceEpoch.toDouble();
      }
      final rawPrice = c[closeIdx];
      final double? price = rawPrice is num
          ? rawPrice.toDouble()
          : double.tryParse(rawPrice.toString());
      if (price == null) continue;
      spots.add(FlSpot(ts, price));
    }
    setState(() => _allSpots = spots);
  }

  DateTime? _parseTradeDate(dynamic dateStr) {
    if (dateStr == null) return null;
    try {
      var s = dateStr.toString().trim().replaceFirst(' ', 'T');
      if (!s.contains('+') && !s.endsWith('Z')) s += 'Z';
      return DateTime.parse(s).toUtc();
    } catch (_) {
      return null;
    }
  }

  Widget _buildChart(BuildContext context) {
    if (_chartError != null) {
      return Center(child: Text('Error: $_chartError', style: const TextStyle(color: Colors.red), textAlign: TextAlign.center));
    }
    if (_isLoadingChart || _candleData == null) {
      return const Center(child: CircularProgressIndicator());
    }

    if (_allSpots.isEmpty) {
      return const Center(child: Text('No candle data available.', style: TextStyle(color: Colors.grey)));
    }

    final total = _allSpots.length;
    final count = math.min(_viewCount, total);
    final start = (total - count - _viewOffset).clamp(0, total - count);
    final spots = _allSpots.sublist(start, start + count);

    final minX = spots.first.x;
    final maxX = spots.last.x;
    final prices = spots.map((s) => s.y);
    final minY = prices.reduce(math.min) * 0.998;
    final maxY = prices.reduce(math.max) * 1.002;

    final xInterval = (maxX - minX) / 4;

    return LineChart(
      LineChartData(
        minX: minX, maxX: maxX, minY: minY, maxY: maxY,
        backgroundColor: const Color(0xFF1E1E1E),
        clipData: const FlClipData.all(),
        lineTouchData: LineTouchData(
          touchTooltipData: LineTouchTooltipData(
            getTooltipItems: (touchedSpots) => touchedSpots.map((s) {
              if (s.barIndex != 0) return null;
              final date = DateTime.fromMillisecondsSinceEpoch(s.x.toInt());
              return LineTooltipItem(
                '${s.y.toStringAsFixed(4)}\n',
                const TextStyle(color: Colors.white, fontWeight: FontWeight.bold, fontSize: 12),
                children: [TextSpan(text: DateFormat('d MMM  HH:mm').format(date), style: const TextStyle(color: Colors.white60, fontSize: 10, fontWeight: FontWeight.normal))],
              );
            }).toList(),
          ),
        ),
        gridData: FlGridData(
          show: true,
          getDrawingHorizontalLine: (_) => const FlLine(color: Color(0xFF2A2A2A), strokeWidth: 0.8),
          getDrawingVerticalLine: (_) => const FlLine(color: Color(0xFF2A2A2A), strokeWidth: 0.8),
        ),
        borderData: FlBorderData(show: true, border: Border.all(color: const Color(0xFF333333))),
        titlesData: FlTitlesData(
          topTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
          rightTitles: const AxisTitles(sideTitles: SideTitles(showTitles: false)),
          leftTitles: AxisTitles(sideTitles: SideTitles(
            showTitles: true, reservedSize: 62,
            getTitlesWidget: (v, meta) {
              if (v == meta.max || v == meta.min) return const SizedBox.shrink();
              return Text(v.toStringAsFixed(2), style: const TextStyle(color: Color(0xFF666666), fontSize: 9));
            },
          )),
          bottomTitles: AxisTitles(sideTitles: SideTitles(
            showTitles: true, reservedSize: 22,
            interval: xInterval > 0 ? xInterval : null,
            getTitlesWidget: (v, meta) {
              if (v == meta.max || v == meta.min) return const SizedBox.shrink();
              final dt = DateTime.fromMillisecondsSinceEpoch(v.toInt());
              return SideTitleWidget(axisSide: meta.axisSide,
                child: Text(DateFormat('d/M HH:mm').format(dt), style: const TextStyle(color: Color(0xFF666666), fontSize: 8)));
            },
          )),
        ),
        lineBarsData: [
          LineChartBarData(
            spots: spots,
            isCurved: false,
            color: const Color(0xFF4FC3F7),
            barWidth: 1.5,
            dotData: const FlDotData(show: false),
            belowBarData: BarAreaData(
              show: true,
              gradient: LinearGradient(
                colors: [const Color(0xFF4FC3F7).withOpacity(0.18), const Color(0xFF4FC3F7).withOpacity(0.0)],
                begin: Alignment.topCenter, end: Alignment.bottomCenter,
              ),
            ),
          ),
          ..._buildTradeBars(minX, maxX),
        ],
      ),
    );
  }

  List<LineChartBarData> _buildTradeBars(double minX, double maxX) {
    final bars = <LineChartBarData>[];

    bool inView(double ts) => ts >= minX && ts <= maxX;

    for (final t in _closedTrades.where((t) => t['pair'] == _selectedPair)) {
      final entryTs   = _parseTradeDate(t['open_date'])?.millisecondsSinceEpoch.toDouble();
      final exitTs    = _parseTradeDate(t['close_date'])?.millisecondsSinceEpoch.toDouble();
      final entryRate = (t['open_rate'] as num?)?.toDouble();
      final exitRate  = (t['close_rate'] as num?)?.toDouble();
      if (entryTs == null || exitTs == null || entryRate == null || exitRate == null) continue;

      final profit   = (t['profit_ratio'] as num?)?.toDouble() ?? 0.0;
      final isShort  = t['is_short'] == true;
      final lineColor = profit >= 0 ? Colors.greenAccent : Colors.redAccent;

      final entryVis = inView(entryTs);
      final exitVis  = inView(exitTs);

      if (entryVis && exitVis) {
        bars.add(LineChartBarData(
          spots: [FlSpot(entryTs, entryRate), FlSpot(exitTs, exitRate)],
          color: lineColor.withOpacity(0.45),
          barWidth: 1.2,
          isCurved: false,
          dotData: FlDotData(
            show: true,
            getDotPainter: (spot, percent, barData, index) => index == 0
                ? _FlDotTrianglePainter(color: lineColor, pointDown: isShort)
                : FlDotCirclePainter(radius: 5, color: lineColor, strokeWidth: 1.5, strokeColor: Colors.black45),
          ),
        ));
      } else if (entryVis) {
        bars.add(_singleDotBar(entryTs, entryRate,
            _FlDotTrianglePainter(color: lineColor, pointDown: isShort)));
      } else if (exitVis) {
        bars.add(_singleDotBar(exitTs, exitRate,
            FlDotCirclePainter(radius: 5, color: lineColor, strokeWidth: 1.5, strokeColor: Colors.black45)));
      }
    }

    for (final t in _openTrades.where((t) => t['pair'] == _selectedPair)) {
      final entryTs   = _parseTradeDate(t['open_date'])?.millisecondsSinceEpoch.toDouble();
      final entryRate = (t['open_rate'] as num?)?.toDouble();
      if (entryTs == null || entryRate == null) continue;
      if (!inView(entryTs)) continue;

      final isShort = t['is_short'] == true;
      bars.add(_singleDotBar(entryTs, entryRate,
          _FlDotTrianglePainter(color: Colors.greenAccent, pointDown: isShort)));
    }

    return bars;
  }

  LineChartBarData _singleDotBar(double ts, double price, FlDotPainter painter) =>
      LineChartBarData(
        spots: [FlSpot(ts, price)],
        color: Colors.transparent,
        barWidth: 0,
        dotData: FlDotData(
          show: true,
          getDotPainter: (_, __, ___, ____) => painter,
        ),
      );

  @override
  Widget build(BuildContext context) {
    super.build(context);

    if (_isLoading) return const Center(child: CircularProgressIndicator());
    if (_error != null) {
      return ErrorView(message: _error!, onRetry: _fetchInitialData);
    }

    final openTradeByPair = <String, Map<String, dynamic>>{};
    for (final t in _openTrades) {
      openTradeByPair[t['pair'] as String] = t as Map<String, dynamic>;
    }

    final allPairs = [
      ..._whitelist,
      ..._openTrades
          .map((t) => t['pair'] as String)
          .where((p) => !_whitelist.contains(p)),
    ];

    return Container(
      color: const Color(0xFF1E1E1E),
      child: Column(
        children: [
          // Pair selector row + refresh button (Bug 4 fix)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 10, 12, 6),
            child: allPairs.isEmpty
                ? const Padding(
                    padding: EdgeInsets.symmetric(vertical: 8),
                    child: Text('No pairs found — check bot connection.', style: TextStyle(color: Colors.white54)),
                  )
                : Row(
                    children: [
                      const Text('Pair', style: TextStyle(color: Color(0xFF888888), fontSize: 12)),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 12),
                          decoration: BoxDecoration(
                            color: const Color(0xFF252525),
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(color: const Color(0xFF4FC3F7), width: 1),
                          ),
                          child: DropdownButtonHideUnderline(
                            child: DropdownButton<String>(
                              value: _selectedPair,
                              isExpanded: true,
                              dropdownColor: const Color(0xFF252525),
                              iconEnabledColor: const Color(0xFF4FC3F7),
                              onChanged: (pair) { if (pair != null) _loadChart(pair); },
                              items: allPairs.map((pair) {
                                final trade = openTradeByPair[pair];
                                final profit = (trade?['profit_ratio'] as num?)?.toDouble();
                                return DropdownMenuItem<String>(
                                  value: pair,
                                  child: Row(
                                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                                    children: [
                                      Text(pair, style: const TextStyle(color: Colors.white, fontSize: 13, fontWeight: FontWeight.bold)),
                                      if (profit != null)
                                        Container(
                                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                          decoration: BoxDecoration(
                                            color: (profit >= 0 ? Colors.green : Colors.red).withOpacity(0.2),
                                            borderRadius: BorderRadius.circular(4),
                                          ),
                                          child: Text(
                                            '${profit >= 0 ? '+' : ''}${(profit * 100).toStringAsFixed(2)}%',
                                            style: TextStyle(
                                              color: profit >= 0 ? Colors.greenAccent : Colors.redAccent,
                                              fontSize: 11,
                                              fontWeight: FontWeight.bold,
                                            ),
                                          ),
                                        ),
                                    ],
                                  ),
                                );
                              }).toList(),
                            ),
                          ),
                        ),
                      ),
                      // Bug 4 fix: permanent refresh button for chart
                      const SizedBox(width: 8),
                      IconButton(
                        icon: const Icon(Icons.refresh, color: Color(0xFF4FC3F7)),
                        tooltip: 'Refresh chart',
                        onPressed: _selectedPair != null ? () => _loadChart(_selectedPair!) : null,
                        padding: EdgeInsets.zero,
                        constraints: const BoxConstraints(),
                      ),
                    ],
                  ),
          ),
          Expanded(
            child: _selectedPair == null
                ? const Center(child: Text('Select a pair above', style: TextStyle(color: Colors.white38)))
                : LayoutBuilder(
                    builder: (context, constraints) {
                      final pixelsPerCandle = constraints.maxWidth / _viewCount;
                      return GestureDetector(
                        onHorizontalDragStart: (d) {
                          _dragAnchorX = d.localPosition.dx;
                          _dragAnchorOffset = _viewOffset;
                        },
                        onHorizontalDragUpdate: (d) {
                          if (_dragAnchorX == null || _dragAnchorOffset == null) return;
                          final dx = d.localPosition.dx - _dragAnchorX!;
                          final shift = (dx / pixelsPerCandle).round();
                          final maxOffset = (_allSpots.length - _viewCount).clamp(0, _allSpots.length);
                          setState(() {
                            _viewOffset = (_dragAnchorOffset! + shift).clamp(0, maxOffset);
                          });
                        },
                        onHorizontalDragEnd: (_) {
                          _dragAnchorX = null;
                          _dragAnchorOffset = null;
                        },
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(4, 0, 12, 4),
                          child: _buildChart(context),
                        ),
                      );
                    },
                  ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 4, 12, 10),
            child: Row(children: [
              _legendLine(const Color(0xFF4FC3F7), 'Price'),
              const SizedBox(width: 14),
              _legendTriangle(Colors.greenAccent, up: true, label: 'Long Entry'),
              const SizedBox(width: 14),
              _legendTriangle(Colors.redAccent, up: false, label: 'Short Entry'),
              const SizedBox(width: 14),
              _legendDot(Colors.greenAccent, 'Win Exit'),
              const SizedBox(width: 14),
              _legendDot(Colors.redAccent, 'Loss Exit'),
            ]),
          ),
        ],
      ),
    );
  }

  Widget _legendLine(Color color, String label) {
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Container(width: 14, height: 2, color: color),
      const SizedBox(width: 4),
      Text(label, style: const TextStyle(color: Color(0xFF666666), fontSize: 9)),
    ]);
  }

  Widget _legendTriangle(Color color, {required bool up, required String label}) {
    return Row(mainAxisSize: MainAxisSize.min, children: [
      CustomPaint(
        size: const Size(10, 10),
        painter: _TrianglePainter(color: color, up: up),
      ),
      const SizedBox(width: 4),
      Text(label, style: const TextStyle(color: Color(0xFF666666), fontSize: 9)),
    ]);
  }

  Widget _legendDot(Color color, String label) {
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Container(
        width: 8, height: 8,
        decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      ),
      const SizedBox(width: 4),
      Text(label, style: const TextStyle(color: Color(0xFF666666), fontSize: 9)),
    ]);
  }
}

class _FlDotTrianglePainter extends FlDotPainter {
  final Color color;
  final bool pointDown;
  final double size;

  const _FlDotTrianglePainter({
    required this.color,
    required this.pointDown,
    this.size = 7.0,
  });

  @override
  void draw(Canvas canvas, FlSpot spot, Offset offsetInCanvas) {
    final paint = Paint()..color = color..style = PaintingStyle.fill;
    final path = Path();
    final c = offsetInCanvas;
    final r = size;
    if (pointDown) {
      path.moveTo(c.dx, c.dy + r);
      path.lineTo(c.dx - r, c.dy - r * 0.65);
      path.lineTo(c.dx + r, c.dy - r * 0.65);
    } else {
      path.moveTo(c.dx, c.dy - r);
      path.lineTo(c.dx - r, c.dy + r * 0.65);
      path.lineTo(c.dx + r, c.dy + r * 0.65);
    }
    path.close();
    canvas.drawPath(path, paint);
  }

  @override
  Size getSize(FlSpot spot) => Size(size * 2, size * 2);

  @override
  Color get mainColor => color;

  @override
  FlDotPainter lerp(FlDotPainter a, FlDotPainter b, double t) => b;

  @override
  bool hitTest(FlSpot spot, Offset touched, Offset center, double extraThreshold) =>
      (touched - center).distance < size + extraThreshold;

  @override
  List<Object?> get props => [color, pointDown, size];
}

class _TrianglePainter extends CustomPainter {
  final Color color;
  final bool up;
  const _TrianglePainter({required this.color, required this.up});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color..style = PaintingStyle.fill;
    final path = Path();
    if (up) {
      path.moveTo(size.width / 2, 0);
      path.lineTo(0, size.height);
      path.lineTo(size.width, size.height);
    } else {
      path.moveTo(size.width / 2, size.height);
      path.lineTo(0, 0);
      path.lineTo(size.width, 0);
    }
    path.close();
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_TrianglePainter old) => old.color != color || old.up != up;
}
