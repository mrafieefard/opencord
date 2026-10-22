import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/features/settings/server/permission_info.dart';

void main() {
  test('every permission bit appears exactly once', () {
    final bits = [
      for (final (_, infos) in permissionGroups)
        for (final info in infos) info.bit,
    ];
    final all = bits.fold(Permissions.none, (sum, bit) => sum | bit);

    expect(bits.toSet(), hasLength(bits.length));
    expect(all, Permissions.all);
  });
}
