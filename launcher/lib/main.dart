import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;

const levelsUrl = 'https://raw.githubusercontent.com/Endlad2/Infinity-Backrooms/main/levels.json';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const InfinityLauncher());
}

class InfinityLauncher extends StatelessWidget {
  const InfinityLauncher({super.key});

  @override
  Widget build(BuildContext context) {
    final scheme = ColorScheme.fromSeed(seedColor: const Color(0xFFC6B04A), brightness: Brightness.dark);
    return MaterialApp(
      title: 'Backrooms Infinity',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(colorScheme: scheme, useMaterial3: true, scaffoldBackgroundColor: const Color(0xFF0E0E0A)),
      home: const HomePage(),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key});
  @override State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  String nickname = 'Wanderer';
  late final TextEditingController nicknameController = TextEditingController(text: nickname);

  @override
  void dispose() { nicknameController.dispose(); super.dispose(); }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Container(
        decoration: const BoxDecoration(gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [Color(0xFF2A2815), Color(0xFF0E0E0A), Color(0xFF17170F)])),
        child: SafeArea(child: Center(child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 900),
          child: Padding(
            padding: const EdgeInsets.all(40),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.center, children: [
              Text('BACKROOMS', style: Theme.of(context).textTheme.displayLarge?.copyWith(fontWeight: FontWeight.w900, letterSpacing: 8)),
              Text('INFINITY', style: Theme.of(context).textTheme.headlineLarge?.copyWith(color: Theme.of(context).colorScheme.primary, letterSpacing: 16)),
              const SizedBox(height: 12),
              const Text('Open-source exploration • Bevy + Rust • LAN multiplayer'),
              const SizedBox(height: 42),
              TextField(decoration: const InputDecoration(labelText: 'Nickname', border: OutlineInputBorder()), controller: nicknameController, onChanged: (v) => nickname = v.trim().isEmpty ? 'Wanderer' : v.trim()),
              const SizedBox(height: 18),
              Row(children: [
                Expanded(child: _BigButton(icon: Icons.person, label: 'Single player', onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => SinglePlayerPage(nickname: nickname))))),
                const SizedBox(width: 16),
                Expanded(child: _BigButton(icon: Icons.lan, label: 'Multiplayer', onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => MultiplayerPage(nickname: nickname))))),
              ]),
              const SizedBox(height: 20),
              Text('Data: ${appDataDir().path}', style: Theme.of(context).textTheme.bodySmall),
            ]),
          ),
        ))),
      ),
    );
  }
}

class _BigButton extends StatelessWidget {
  final IconData icon; final String label; final VoidCallback onTap;
  const _BigButton({required this.icon, required this.label, required this.onTap});
  @override Widget build(BuildContext context) => FilledButton.tonal(onPressed: onTap, style: FilledButton.styleFrom(padding: const EdgeInsets.symmetric(vertical: 28)), child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [Icon(icon), const SizedBox(width: 12), Text(label)]));
}

class LevelInfo {
  final String name; final String url;
  const LevelInfo(this.name, this.url);
  factory LevelInfo.fromJson(Map<String, dynamic> j) => LevelInfo(j['name'] as String, j['url'] as String);
  String get id => name.toLowerCase().replaceAll(RegExp(r'[^a-z0-9]+'), '-').replaceAll(RegExp(r'^-|-$'), '');
}

Future<List<LevelInfo>> fetchLevels() async {
  try {
    final response = await http.get(Uri.parse(levelsUrl)).timeout(const Duration(seconds: 10));
    if (response.statusCode == 200) {
      final raw = jsonDecode(response.body) as List<dynamic>;
      return raw.map((e) => LevelInfo.fromJson(e as Map<String, dynamic>)).toList();
    }
  } catch (_) {}
  final local = File(p.join(appDataDir().path, 'levels.json'));
  if (await local.exists()) {
    final raw = jsonDecode(await local.readAsString()) as List<dynamic>;
    return raw.map((e) => LevelInfo.fromJson(e as Map<String, dynamic>)).toList();
  }
  return [];
}

class SinglePlayerPage extends StatefulWidget {
  final String nickname; const SinglePlayerPage({super.key, required this.nickname});
  @override State<SinglePlayerPage> createState() => _SinglePlayerPageState();
}

class _SinglePlayerPageState extends State<SinglePlayerPage> {
  late Future<List<LevelInfo>> levels = fetchLevels();
  String status = '';
  @override Widget build(BuildContext context) => Scaffold(appBar: AppBar(title: const Text('Select level')), body: Padding(padding: const EdgeInsets.all(24), child: FutureBuilder<List<LevelInfo>>(
    future: levels,
    builder: (context, snapshot) {
      if (snapshot.connectionState != ConnectionState.done) return const Center(child: CircularProgressIndicator());
      final data = snapshot.data ?? const <LevelInfo>[];
      if (data.isEmpty) return Center(child: Column(mainAxisSize: MainAxisSize.min, children: [const Icon(Icons.layers_clear, size: 60), const SizedBox(height: 16), const Text('levels.json is empty. Levels will appear here after they are added.'), const SizedBox(height: 16), FilledButton(onPressed: () => setState(() => levels = fetchLevels()), child: const Text('Refresh')), if (status.isNotEmpty) Text(status)]));
      return ListView(children: [for (final level in data) Card(child: ListTile(title: Text(level.name), subtitle: Text(level.url), trailing: FilledButton(onPressed: () => playSingle(level), child: const Text('Play')))), if (status.isNotEmpty) Padding(padding: const EdgeInsets.all(12), child: Text(status))]);
    },
  )));

  Future<void> playSingle(LevelInfo level) async {
    try {
      setState(() => status = 'Preparing ${level.name}…');
      final dir = await ensureLevel(level);
      await startClient(mode: 'singleplayer', nickname: widget.nickname, level: dir.path);
      if (mounted) setState(() => status = 'Game started');
    } catch (e) { if (mounted) setState(() => status = 'Error: $e'); }
  }
}

class MultiplayerPage extends StatelessWidget {
  final String nickname; const MultiplayerPage({super.key, required this.nickname});
  @override Widget build(BuildContext context) => Scaffold(appBar: AppBar(title: const Text('Multiplayer')), body: Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 680), child: Padding(padding: const EdgeInsets.all(24), child: Column(mainAxisSize: MainAxisSize.min, children: [
    const Text('LAN multiplayer • maximum 5 players'), const SizedBox(height: 24),
    _BigButton(icon: Icons.wifi_tethering, label: 'I am the host', onTap: () => Navigator.push(context, MaterialPageRoute(builder: (_) => HostPage(nickname: nickname)))),
    const SizedBox(height: 16),
    _BigButton(icon: Icons.login, label: 'Connect to host', onTap: () => Navigator.push(context, MaterialPageRoute(builder: (_) => JoinPage(nickname: nickname)))),
  ])))));
}

class HostPage extends StatefulWidget {
  final String nickname; const HostPage({super.key, required this.nickname});
  @override State<HostPage> createState() => _HostPageState();
}

class _HostPageState extends State<HostPage> {
  Process? sidecar; String ip = 'starting…'; int players = 1; String status = ''; List<LevelInfo> levels = []; LevelInfo? selected;
  StreamSubscription<String>? outSub;
  @override void initState() { super.initState(); _start(); }
  @override void dispose() { outSub?.cancel(); sidecar?.kill(); super.dispose(); }

  Future<void> _start() async {
    levels = await fetchLevels(); selected = levels.isNotEmpty ? levels.first : null;
    if (mounted) setState(() {});
    try {
      sidecar = await Process.start(binaryPath('bi-lan'), ['host', '--nickname', widget.nickname]);
      outSub = sidecar!.stdout.transform(utf8.decoder).transform(const LineSplitter()).listen((line) {
        final e = jsonDecode(line) as Map<String, dynamic>;
        if (!mounted) return;
        setState(() {
          if (e['event'] == 'host_ready') { ip = e['ip'] as String; status = 'Lobby ready'; }
          if (e['event'] == 'player_count') players = e['players'] as int;
          if (e['event'] == 'start') status = 'Game starting…';
        });
      });
      sidecar!.stderr.transform(utf8.decoder).listen((s) { if (mounted && s.trim().isNotEmpty) setState(() => status = s.trim()); });
    } catch (e) { if (mounted) setState(() => status = 'Could not start bi-lan: $e'); }
  }

  Future<void> _startGame() async {
    final level = selected; if (level == null || sidecar == null) return;
    try {
      final dir = await ensureLevel(level);
      sidecar!.stdin.writeln(jsonEncode({'command':'start','level':level.id}));
      await sidecar!.stdin.flush();
      await startClient(mode: 'multiplayer', nickname: widget.nickname, level: dir.path);
      if (mounted) setState(() => status = 'Started');
    } catch (e) { if (mounted) setState(() => status = 'Error: $e'); }
  }

  @override Widget build(BuildContext context) => Scaffold(appBar: AppBar(title: const Text('Host lobby')), body: Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 650), child: Padding(padding: const EdgeInsets.all(24), child: Column(mainAxisSize: MainAxisSize.min, children: [
    Text('Host IP', style: Theme.of(context).textTheme.titleMedium), SelectableText(ip, style: Theme.of(context).textTheme.displaySmall), const SizedBox(height: 12), Text('Players: $players / 5'), const SizedBox(height: 24),
    if (levels.isEmpty) const Text('No levels in levels.json — Start is disabled.') else DropdownButton<LevelInfo>(value: selected, isExpanded: true, items: [for (final l in levels) DropdownMenuItem(value:l, child:Text(l.name))], onChanged:(v)=>setState(()=>selected=v)),
    const SizedBox(height: 16), FilledButton.icon(onPressed: selected == null ? null : _startGame, icon: const Icon(Icons.play_arrow), label: const Text('Start')), const SizedBox(height: 12), Text(status),
  ])))));
}

class JoinPage extends StatefulWidget {
  final String nickname; const JoinPage({super.key, required this.nickname});
  @override State<JoinPage> createState() => _JoinPageState();
}

class _JoinPageState extends State<JoinPage> {
  final host = TextEditingController(); Process? sidecar; String status = 'Enter host IP'; bool connected = false;
  StreamSubscription<String>? outSub;
  @override void dispose() { outSub?.cancel(); sidecar?.kill(); host.dispose(); super.dispose(); }

  Future<void> connect() async {
    sidecar?.kill(); connected = false; setState(() => status = 'Checking host…');
    try {
      sidecar = await Process.start(binaryPath('bi-lan'), ['join','--host',host.text.trim(),'--nickname',widget.nickname]);
      outSub = sidecar!.stdout.transform(utf8.decoder).transform(const LineSplitter()).listen((line) async {
        final e = jsonDecode(line) as Map<String, dynamic>;
        if (e['event'] == 'connected') { if (mounted) setState(() { connected = true; status = 'Connected. Waiting for host to press Start…'; }); }
        if (e['event'] == 'player_count' && mounted) setState(() => status = 'Connected: ${e['players']}/5. Waiting for Start…');
        if (e['event'] == 'start') {
          final id = e['level'] as String;
          if (mounted) setState(() => status = 'Host started the game. Preparing level…');
          final levels = await fetchLevels();
          LevelInfo? match;
          for (final level in levels) { if (level.id == id) { match = level; break; } }
          String levelArg = id;
          if (match != null) levelArg = (await ensureLevel(match)).path;
          await startClient(mode: 'multiplayer', nickname: widget.nickname, level: levelArg);
          if (mounted) setState(() => status = 'Game started');
        }
      });
      sidecar!.stderr.transform(utf8.decoder).listen((s) { if (mounted && s.trim().isNotEmpty) setState(() => status = s.trim()); });
      unawaited(sidecar!.exitCode.then((code) { if (mounted && !connected) setState(() => status = 'Connection failed (bi-lan exit $code)'); }));
    } catch (e) { if (mounted) setState(() => status = 'Could not start bi-lan: $e'); }
  }

  @override Widget build(BuildContext context) => Scaffold(appBar: AppBar(title: const Text('Join LAN game')), body: Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 560), child: Padding(padding: const EdgeInsets.all(24), child: Column(mainAxisSize: MainAxisSize.min, children: [
    TextField(controller: host, decoration: const InputDecoration(labelText: 'Host IP', hintText: '192.168.1.50', border: OutlineInputBorder())), const SizedBox(height: 16), FilledButton.icon(onPressed: connect, icon: const Icon(Icons.link), label: const Text('Connect')), const SizedBox(height: 20), Text(status, textAlign: TextAlign.center),
  ])))));
}

Directory appDataDir() {
  if (Platform.isWindows) {
    final base = Platform.environment['APPDATA'] ?? Platform.environment['USERPROFILE'] ?? Directory.current.path;
    return Directory(p.join(base, '.backrooms_infinity'));
  }
  final home = Platform.environment['HOME'] ?? Directory.current.path;
  return Directory(p.join(home, '.backrooms_infinity'));
}

String binaryPath(String name) {
  final exe = Platform.isWindows ? '$name.exe' : name;
  final installed = File(p.join(appDataDir().path, 'bin', exe));
  if (installed.existsSync()) return installed.path;
  final candidates = [
    p.absolute('..', 'target', 'release', exe),
    p.absolute('target', 'release', exe),
    p.join(Directory.current.path, exe),
  ];
  return candidates.firstWhere((c) => File(c).existsSync(), orElse: () => installed.path);
}

Future<Directory> ensureLevel(LevelInfo level) async {
  final root = Directory(p.join(appDataDir().path, 'levels', level.id));
  final marker = File(p.join(root.path, '.installed'));
  final manifest = File(p.join(root.path, 'level.xml'));
  if (await marker.exists() && await manifest.exists()) {
    final installedFrom = await marker.readAsString();
    if (installedFrom.trim() == level.url) return root;
  }

  if (await root.exists()) await root.delete(recursive: true);
  await root.create(recursive: true);
  final response = await http.get(Uri.parse(level.url)).timeout(const Duration(minutes: 2));
  if (response.statusCode != 200) throw Exception('Level download failed: HTTP ${response.statusCode}');
  final archive = ZipDecoder().decodeBytes(Uint8List.fromList(response.bodyBytes));
  for (final entry in archive) {
    final normalized = p.normalize(entry.name);
    if (p.isAbsolute(normalized) || normalized == '..' || normalized.startsWith('../') || normalized.startsWith('..${p.separator}')) {
      throw Exception('Unsafe path in level archive: ${entry.name}');
    }
    final out = p.join(root.path, normalized);
    if (entry.isFile) {
      final file = File(out);
      await file.parent.create(recursive: true);
      await file.writeAsBytes(entry.content as List<int>);
    } else {
      await Directory(out).create(recursive: true);
    }
  }
  if (!await manifest.exists()) {
    await root.delete(recursive: true);
    throw Exception('Invalid level archive: level.xml must be at the ZIP root');
  }
  await marker.writeAsString(level.url);
  return root;
}

Future<Process> startClient({required String mode, required String nickname, required String level}) async {
  return Process.start(binaryPath('client'), ['--mode', mode, '--nickname', nickname, '--level', level, '--bridge-ws', 'ws://127.0.0.1:37610/client'], mode: ProcessStartMode.detached);
}
