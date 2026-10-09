import 'dart:io';

/// Diagnostico temporario do pane (build de teste) — grava em arquivo o que o
/// painel realmente recebe/pinta, para separar bug de modelo x render.
/// Escreve em DOIS caminhos (Public e TEMP) para nao depender do perfil.
/// REMOVER depois do diagnostico.
int _anetDiagCount = 0;

void anetDiag(String s) {
  if (_anetDiagCount > 4000) return;
  _anetDiagCount++;
  final linha =
      '${DateTime.now().toIso8601String()} #$_anetDiagCount pid=$pid $s\n';
  final alvos = <String>[
    'C:\\Users\\Public\\anet_rd_diag.log',
    '${Directory.systemTemp.path}${Platform.pathSeparator}anet_diag.log',
  ];
  for (final p in alvos) {
    try {
      File(p).writeAsStringSync(linha, mode: FileMode.append, flush: true);
    } catch (_) {}
  }
}
