// Сборка команды и запуск Rust-клиента (§2.1 ТЗ).
// Формирует команды вида:
//   client.exe --mode single --level 0
//   client.exe --mode host --level 5
//   client.exe --mode join --ip 192.168.1.42
//
// Бинарник теперь берётся из %APPDATA%/.infinity-backrooms/client.exe

import 'dart:io';

import 'data/paths.dart';

/// Режим запуска клиента.
enum RunMode { single, host, join }

/// Результат запуска: успех или ошибка.
class RunResult {
  final bool ok;
  final String message;
  RunResult(this.ok, this.message);
}

/// Собирает список аргументов для клиента.
List<String> buildClientArgs({
  required RunMode mode,
  int? level,
  String? ip,
}) {
  final args = <String>[];
  switch (mode) {
    case RunMode.single:
      args.addAll(['--mode', 'single']);
      if (level != null) {
        args.addAll(['--level', level.toString()]);
      }
      break;
    case RunMode.host:
      args.addAll(['--mode', 'host']);
      if (level != null) {
        args.addAll(['--level', level.toString()]);
      }
      break;
    case RunMode.join:
      args.addAll(['--mode', 'join']);
      if (ip != null && ip.isNotEmpty) {
        args.addAll(['--ip', ip]);
      }
      break;
  }
  return args;
}

/// Имя бинарника клиента под текущую ОС.
String clientBinaryName() {
  if (Platform.isWindows) return 'client.exe';
  return 'client';
}

/// Путь к бинарнику клиента: %APPDATA%/.infinity-backrooms/client.exe
Future<String?> findClientBinary() async {
  final paths = AppPaths.resolve();
  final binary = File(AppPaths.join(paths.root.path, clientBinaryName()));
  if (await binary.exists()) {
    return binary.path;
  }
  // Fallback: возможно клиент лежит рядом с лаунчером.
  final fallback = File(clientBinaryName());
  if (await fallback.exists()) return fallback.path;
  return null;
}

/// Запускает клиент с заданными параметрами.
Future<RunResult> launchClient({
  required RunMode mode,
  int? level,
  String? ip,
  bool detached = false,
}) async {
  final binPath = await findClientBinary();
  if (binPath == null) {
    return RunResult(
      false,
      'Не найден client.exe в %APPDATA%\\.infinity-backrooms\\.\n'
      'Соберите Rust-клиент и положите его туда.',
    );
  }
  final args = buildClientArgs(mode: mode, level: level, ip: ip);
  try {
    if (detached) {
      await Process.start(
        binPath,
        args,
        mode: ProcessStartMode.detached,
        workingDirectory: File(binPath).parent.path,
      );
    } else {
      final proc = await Process.start(
        binPath,
        args,
        workingDirectory: File(binPath).parent.path,
      );
      await proc.stdout.drain<void>();
      await proc.stderr.drain<void>();
      final code = await proc.exitCode;
      if (code != 0) {
        return RunResult(false, 'Клиент завершился с кодом $code');
      }
    }
    return RunResult(true, 'OK');
  } catch (e) {
    return RunResult(false, 'Ошибка запуска: $e');
  }
}
