import 'package:flutter/material.dart';

class OfflineBanner extends StatelessWidget {
  final DateTime? lastSynced;

  const OfflineBanner({super.key, this.lastSynced});

  String _timeAgo(DateTime dt) {
    final diff = DateTime.now().difference(dt);
    if (diff.inMinutes < 1) return 'just now';
    if (diff.inHours < 1) return '${diff.inMinutes}m ago';
    if (diff.inDays < 1) return '${diff.inHours}h ago';
    return '${diff.inDays}d ago';
  }

  @override
  Widget build(BuildContext context) {
    final syncText = lastSynced != null ? 'Last synced ${_timeAgo(lastSynced!)}' : 'No sync time recorded';
    return Container(
      width: double.infinity,
      color: Colors.amber.shade700,
      padding: const EdgeInsets.symmetric(vertical: 5, horizontal: 12),
      child: Row(
        children: [
          const Icon(Icons.cloud_off, size: 14, color: Colors.white),
          const SizedBox(width: 6),
          Text(
            'Offline · $syncText',
            style: const TextStyle(color: Colors.white, fontSize: 12, fontWeight: FontWeight.w600),
          ),
        ],
      ),
    );
  }
}
