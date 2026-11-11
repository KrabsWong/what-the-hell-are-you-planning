// Development-only oracle. Node 24+ and an external Beautiful Mermaid 1.1.3
// installation are required only when refreshing fixtures, never to build Rust.
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
const [packageRoot, fixtureDir] = process.argv.slice(2);
if (!packageRoot || !fixtureDir) throw new Error('Usage: node generate_reference.mjs BEAUTIFUL_MERMAID_PACKAGE_DIR FIXTURE_DIR');
const root = path.resolve(packageRoot);
const metadata = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
if (metadata.version !== '1.1.3') throw new Error('Expected Beautiful Mermaid 1.1.3');
const require = createRequire(path.join(root, 'package.json'));
const elkMetadata = JSON.parse(await fs.readFile(require.resolve('elkjs/package.json'), 'utf8'));
if (elkMetadata.version !== '0.11.1') throw new Error('Expected ELK.js 0.11.1');
const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'beautiful-mermaid-oracle-'));
await fs.writeFile(path.join(temp, 'package.json'), '{"type":"module"}');
async function copy(relative = '') {
  const destination = path.join(temp, relative);
  await fs.mkdir(destination, { recursive: true });
  for (const item of await fs.readdir(path.join(root, 'src', relative), { withFileTypes: true })) {
    if (item.name === '__tests__') continue;
    const rel = path.join(relative, item.name);
    if (item.isDirectory()) { await copy(rel); continue; }
    if (!item.name.endsWith('.ts')) continue;
    let text = await fs.readFile(path.join(root, 'src', rel), 'utf8');
    text = text.replace(/(from\s+['"])(\.[^'"]+)(['"])/g, (_, a, specifier, b) => a + (specifier.endsWith('.ts') ? specifier : specifier + '.ts') + b);
    for (const dependency of ['elkjs/lib/elk.bundled.js', 'entities']) {
      text = text.replaceAll(`from '${dependency}'`, `from '${pathToFileURL(require.resolve(dependency)).href}'`);
    }
    await fs.writeFile(path.join(temp, rel), text);
  }
}
try {
  await copy();
  const { parseMermaid, renderMermaidSVG } = await import(pathToFileURL(path.join(temp, 'index.ts')));
  const { convertToElkFormat, layoutGraphSync } = await import(pathToFileURL(path.join(temp, 'layout-engine.ts')));
  const { elkLayoutSync } = await import(pathToFileURL(path.join(temp, 'elk-instance.ts')));
  const { THEMES } = await import(pathToFileURL(path.join(temp, 'theme.ts')));
  const fixtureRoot = path.resolve(fixtureDir);
  for (const file of (await fs.readdir(fixtureRoot)).filter(f => f.endsWith('.mmd')).sort()) {
    const source = await fs.readFile(path.join(fixtureRoot, file), 'utf8');
    const header = source.split('\n').map(s=>s.trim()).find(s=>s && !s.startsWith('%%'));
    const name = file.slice(0, -4);
    const output = path.join(fixtureRoot, 'reference');
    await fs.mkdir(output, { recursive: true });
    let result;
    const families = [
      [/^sequenceDiagram$/i, 'sequence', 'parseSequenceDiagram', 'layoutSequenceDiagram'],
      [/^classDiagram$/i, 'class', 'parseClassDiagram', 'layoutClassDiagramSync'],
      [/^erDiagram$/i, 'er', 'parseErDiagram', 'layoutErDiagramSync'],
      [/^xychart(?:-beta)?(?:\s|$)/i, 'xychart', 'parseXYChart', 'layoutXYChart'],
    ];
    const family = families.find(([pattern])=>pattern.test(header));
    if (family) {
      const [, directory, parseName, layoutName] = family;
      const parser = await import(pathToFileURL(path.join(temp, directory, 'parser.ts')));
      const layouter = await import(pathToFileURL(path.join(temp, directory, 'layout.ts')));
      const lines = source.split('\n').map(s=>s.trim()).filter(s=>s && !s.startsWith('%%'));
      const graph = parser[parseName](lines);
      result = { graph, positioned: layouter[layoutName](graph, {}) };
    } else {
      const graph = parseMermaid(source);
      const input = convertToElkFormat(graph);
      result = {
        graph: { ...graph, nodes: [...graph.nodes.values()], classDefs: Object.fromEntries(graph.classDefs),
          classAssignments: Object.fromEntries(graph.classAssignments), nodeStyles: Object.fromEntries(graph.nodeStyles), linkStyles: Object.fromEntries(graph.linkStyles) },
        input, raw: elkLayoutSync(structuredClone(input)), positioned: layoutGraphSync(graph),
      };
    }
    await fs.writeFile(path.join(output, name + '.json'), JSON.stringify({
      upstream: 'beautiful-mermaid@1.1.3 / elkjs@0.11.1', ...result,
    }, null, 2) + '\n');
    // Browser-oriented SVGs retain upstream CSS variables; visual QA resolves them.
    for (const name of ['zinc-light', 'zinc-dark']) {
      await fs.writeFile(path.join(output, file.slice(0, -4) + '.' + name + '.svg'), renderMermaidSVG(source, THEMES[name]));
    }
    console.log(`Generated reference: ${file}`);
  }
} finally { await fs.rm(temp, { recursive: true, force: true }); }
