// Build the WASM playground bundle before Astro builds the site.
//
// Runs `wasm-pack build --target web` on crates/dusrelimage-wasm and writes
// the output to website/src/lib/wasm-pkg/ (gitignored), then copies it to
// website/public/wasm/ so the playground can lazy-load it as a static file.
//
// This script FAILS LOUDLY on purpose: if wasm-pack is missing or the
// build fails, `npm run build` must stop here. Shipping a docs site whose
// playground silently does nothing would be worse than a failed build.
import { execSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const websiteDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = join(websiteDir, '..');
const outDir = join(websiteDir, 'src', 'lib', 'wasm-pkg');
const publicDir = join(websiteDir, 'public', 'wasm');

function fail(message) {
	console.error(`\n[build-wasm] FATAL: ${message}\n`);
	process.exit(1);
}

let wasmPack = 'wasm-pack';
try {
	execSync('wasm-pack --version', { stdio: 'pipe' });
} catch {
	// wasm-pack usually lives in ~/.cargo/bin, which is not always on PATH.
	const home = process.env.HOME;
	const candidate = home ? join(home, '.cargo', 'bin', 'wasm-pack') : '';
	if (candidate && existsSync(candidate)) {
		wasmPack = candidate;
	} else {
		fail(
			'wasm-pack is not installed or not on PATH. Install it (https://rustwasm.github.io/wasm-pack/installer/) and re-run `npm run build`. The playground cannot be built without it.',
		);
	}
}

console.log(`[build-wasm] Building WASM with ${wasmPack} ...`);
try {
	execSync(
		`"${wasmPack}" build --target web crates/dusrelimage-wasm --out-dir "${outDir}"`,
		{ cwd: repoRoot, stdio: 'inherit', env: { ...process.env, PATH: `${process.env.HOME ?? ''}/.cargo/bin:${process.env.PATH ?? ''}` } },
	);
} catch {
	fail('wasm-pack build failed. See the output above — fix the WASM crate, do not ship a broken playground.');
}

const js = join(outDir, 'dusrelimage_wasm.js');
const wasm = join(outDir, 'dusrelimage_wasm_bg.wasm');
if (!existsSync(js) || !existsSync(wasm)) {
	fail(`wasm-pack finished but expected output is missing: ${js} / ${wasm}`);
}

mkdirSync(publicDir, { recursive: true });
for (const file of readdirSync(outDir)) {
	copyFileSync(join(outDir, file), join(publicDir, file));
}
console.log('[build-wasm] WASM ready: src/lib/wasm-pkg/ (+ copy in public/wasm/).');
