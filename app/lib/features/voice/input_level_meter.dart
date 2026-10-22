import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// The quietest level the meter shows; it fills evenly in dB from there to
/// 0 dBFS.
const _floorDbfs = -60.0;

/// The meters showing the microphone: voice media reports the level only
/// while one is open.
class LevelMeters {
  LevelMeters(this._repository);

  final OpencordRepository _repository;
  var _open = 0;

  void open() {
    if (_open++ == 0) _repository.setLevelMeter(true);
  }

  void close() {
    if (--_open == 0) _repository.setLevelMeter(false);
  }
}

final levelMetersProvider = Provider<LevelMeters>(
  (ref) => LevelMeters(ref.watch(repositoryProvider)),
);

/// The microphone's live level (§17.1), after processing and the input
/// volume: what voice activity hears.
class InputLevelMeter extends ConsumerStatefulWidget {
  const InputLevelMeter({super.key});

  @override
  ConsumerState<InputLevelMeter> createState() => _InputLevelMeterState();
}

class _InputLevelMeterState extends ConsumerState<InputLevelMeter> {
  late final LevelMeters _meters;

  /// Null until a level arrives after the meter opened.
  double? _level;

  @override
  void initState() {
    super.initState();
    _meters = ref.read(levelMetersProvider)..open();
  }

  @override
  void dispose() {
    _meters.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(inputLevelProvider, (_, level) {
      if (level != null) setState(() => _level = level);
    });
    final colors = context.oc;
    final level = _level;
    final filled = level == null
        ? 0.0
        : ((level - _floorDbfs) / -_floorDbfs).clamp(0.0, 1.0);
    return Semantics(
      label: 'Input level',
      child: Container(
        height: 4,
        margin: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: colors.selected,
          borderRadius: BorderRadius.circular(2),
        ),
        alignment: Alignment.centerLeft,
        child: FractionallySizedBox(
          widthFactor: filled,
          child: Container(
            decoration: BoxDecoration(
              color: colors.text,
              borderRadius: BorderRadius.circular(2),
            ),
          ),
        ),
      ),
    );
  }
}
