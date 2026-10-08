import 'dart:io';

/// Diagnostico temporario (build v14) — grava em arquivo o que o pane
/// realmente recebe/pinta, para separar bug de modelo/timer x render.
/// REMOVER depois do diagnostico.
int _anetDiagCount = 0;

void anetDiag(String s) {
  try {
    if (_anetDiagCount > 800) return;
    _anetDiagCount++;
    final f = File('${Directory.systemTemp.path}\\anet_diag.log');
    f.writeAsStringSync(
        '${DateTime.now().toIso8601String()} #$_anetDiagCount $s\n',
        mode: FileMode.append,
        flush: true);
  } catch (_) {}
}
