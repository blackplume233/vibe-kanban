import { readFileSync, writeFileSync } from 'node:fs';

const presets = ['verdigris', 'amber'];
const densities = ['comfortable', 'compact'];
const appearances = ['light', 'dark'];
const sizes = ['meta', 'body', 'title', 'page'];
const colorKeys = [
  'canvas', 'rail', 'panel', 'popover', 'hover', 'ink', 'muted', 'divider',
  'controlEdge', 'info', 'ok', 'warn', 'error',
];

function requireValue(condition, path, expected) {
  if (!condition) throw new Error(`${path}: ${expected}`);
}

function parseJson(text) {
  const data = JSON.parse(text);
  // JSON.parse validates syntax first. Scan only strings and object delimiters
  // to detect duplicate keys, including equivalent Unicode escapes.
  const keys = [];
  for (const [token, colon] of text.matchAll(/"(?:[^"\\]|\\.)*"\s*(:)?|[{}]/g)) {
    if (token === '{') keys.push(new Set());
    else if (token === '}') keys.pop();
    else if (colon) {
      const key = JSON.parse(token.slice(0, -1));
      const objectKeys = keys.at(-1);
      requireValue(!objectKeys.has(key), 'tokens.json', `键 ${JSON.stringify(key)} 重复`);
      objectKeys.add(key);
    }
  }
  return data;
}

function fields(value, keys, path) {
  requireValue(value !== null && typeof value === 'object' && !Array.isArray(value), path, '需要对象');
  for (const key of keys) requireValue(Object.hasOwn(value, key), `${path}.${key}`, '缺少字段');
  for (const key of Object.keys(value)) requireValue(keys.includes(key), `${path}.${key}`, '未知字段');
}

function number(value, path, min, max = Infinity) {
  requireValue(
    typeof value === 'number' && Number.isFinite(Math.fround(value)) && value >= min && value <= max,
    path,
    `需要 ${min} 到 ${max === Infinity ? 'f32 上限' : max} 之间的数值`,
  );
}

function colorPairs(value, keys, path) {
  fields(value, keys, path);
  for (const key of keys) {
    const pair = value[key];
    requireValue(
      Array.isArray(pair) && pair.length === 2 && pair.every((c) => typeof c === 'string' && /^#[\da-f]{6}$/i.test(c)),
      `${path}.${key}`,
      '需要按亮色、暗色顺序排列的两个 #RRGGBB 颜色',
    );
  }
}

function validate(tokens) {
  fields(tokens, ['schemaVersion', 'defaults', 'colors', 'accent', 'space', 'type', 'shape', 'size', 'density', 'motion'], 'tokens.json');
  const { defaults, colors, accent, space, type, shape, size, density, motion } = tokens;
  requireValue(tokens.schemaVersion === 1, 'schemaVersion', '需要版本 1');
  fields(defaults, ['appearance', 'accent', 'density'], 'defaults');
  requireValue(['system', ...appearances].includes(defaults.appearance), 'defaults.appearance', '需要 system、light 或 dark');
  requireValue(presets.includes(defaults.accent), 'defaults.accent', '需要 verdigris 或 amber');
  requireValue(densities.includes(defaults.density), 'defaults.density', '需要 comfortable 或 compact');

  colorPairs(colors, colorKeys, 'colors');
  fields(accent, presets, 'accent');
  for (const preset of presets) colorPairs(accent[preset], ['fill', 'onFill', 'selection'], `accent.${preset}`);

  fields(space, ['step'], 'space');
  number(space.step, 'space.step', 1);
  fields(type, ['body', 'mono', 'scale', 'weights'], 'type');
  for (const [key, generic] of [['body', 'sans-serif'], ['mono', 'monospace']]) {
    const chain = type[key];
    const named = (family) => typeof family === 'string' && family === family.trim()
      && /^[^,"'\r\n]+$/.test(family)
      && !/^(sans-serif|serif|monospace|system-ui|ui-(sans-serif|serif|monospace)|-apple-system|BlinkMacSystemFont)$/i.test(family);
    requireValue(
      Array.isArray(chain) && chain.length >= 2 && chain.at(-1) === generic && chain.slice(0, -1).every(named),
      `type.${key}`,
      `需要具名字体列表，末尾为 ${generic}`,
    );
  }
  fields(type.scale, sizes, 'type.scale');
  for (const key of sizes) {
    const entry = type.scale[key];
    requireValue(Array.isArray(entry) && entry.length === 2, `type.scale.${key}`, '需要字号与行高倍率');
    number(entry[0], `type.scale.${key}[0]`, 1);
    number(entry[1], `type.scale.${key}[1]`, 1);
  }
  fields(type.weights, ['regular', 'emphasis'], 'type.weights');
  for (const key of ['regular', 'emphasis']) {
    const weight = type.weights[key];
    number(weight, `type.weights.${key}`, 1, 1000);
    requireValue(Number.isInteger(weight), `type.weights.${key}`, '需要整数');
  }

  const shapeKeys = ['control', 'card', 'overlay', 'focusWidth', 'focusOffset'];
  fields(shape, shapeKeys, 'shape');
  for (const key of shapeKeys) number(shape[key], `shape.${key}`, key === 'focusWidth' ? 2 : 0);
  const sizeKeys = ['split', 'wide', 'rail', 'detail'];
  fields(size, sizeKeys, 'size');
  for (const key of sizeKeys) number(size[key], `size.${key}`, 1);
  requireValue(size.wide > size.split, 'size.wide', '需大于 size.split');

  const densityKeys = ['rowMin', 'controlMin', 'statusMin', 'gutter'];
  fields(density, densities, 'density');
  for (const key of densities) {
    fields(density[key], densityKeys, `density.${key}`);
    for (const field of densityKeys) number(density[key][field], `density.${key}.${field}`, 1);
  }
  for (const key of densityKeys) {
    requireValue(density.compact[key] <= density.comfortable[key], `density.compact.${key}`, '不能大于舒适密度的同一字段');
  }

  const times = ['feedbackMs', 'overlayMs', 'panelMs'];
  fields(motion, [...times, 'reducedMs', 'easeOut'], 'motion');
  for (const key of times) number(motion[key], `motion.${key}`, 0);
  requireValue(motion.reducedMs === 0, 'motion.reducedMs', '需要为零');
  requireValue(Array.isArray(motion.easeOut) && motion.easeOut.length === 4, 'motion.easeOut', '需要四个控制参数');
  for (const [i, point] of motion.easeOut.entries()) number(point, `motion.easeOut[${i}]`, 0, 1);
}

function luminance(hex) {
  const weights = [0.2126, 0.7152, 0.0722];
  return weights.reduce((sum, weight, i) => {
    const value = Number.parseInt(hex.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    const linear = value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    return sum + weight * linear;
  }, 0);
}

function contrast(ink, surface, inkName, surfaceName, minimum, context) {
  const a = luminance(ink);
  const b = luminance(surface);
  const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  requireValue(ratio >= minimum, `${context}.${inkName}/${surfaceName}`, `对比度 ${ratio.toFixed(2)}:1，至少需要 ${minimum}:1`);
}

function projectTheme(tokens, preset) {
  const c = tokens.colors;
  const { fill, onFill, selection } = tokens.accent[preset];
  const colorRoles = {
    background: c.canvas,
    foreground: c.ink,
    card: c.panel,
    'card-foreground': c.ink,
    popover: c.popover,
    'popover-foreground': c.ink,
    primary: fill,
    'primary-foreground': onFill,
    secondary: c.hover,
    'secondary-foreground': c.ink,
    muted: c.hover,
    'muted-foreground': c.muted,
    accent: selection,
    'accent-foreground': c.ink,
    destructive: c.error,
    border: c.divider,
    input: c.controlEdge,
    ring: fill,
    sidebar: c.rail,
    'sidebar-foreground': c.ink,
    'sidebar-primary': fill,
    'sidebar-primary-foreground': onFill,
    'sidebar-accent': selection,
    'sidebar-accent-foreground': c.ink,
    'sidebar-border': c.divider,
    'sidebar-ring': fill,
    'surface-hover': c.hover,
    'status-info': c.info,
    'status-ok': c.ok,
    'status-warn': c.warn,
    'status-error': c.error,
  };
  const cssVars = {
    theme: {
      radius: `${tokens.shape.control}px`,
      'font-sans': tokens.type.body[0],
      'font-mono': tokens.type.mono[0],
    },
  };
  for (const [index, appearance] of appearances.entries()) {
    const roles = Object.fromEntries(Object.entries(colorRoles).map(([name, pair]) => [name, pair[index]]));
    const context = `${preset}.${appearance}`;
    for (const surface of ['background', 'sidebar', 'card', 'popover', 'surface-hover', 'accent']) {
      for (const ink of ['foreground', 'muted-foreground', 'primary', 'status-info', 'status-ok', 'status-warn', 'status-error']) {
        contrast(roles[ink], roles[surface], ink, surface, 4.5, context);
      }
      for (const ink of ['ring', 'input']) contrast(roles[ink], roles[surface], ink, surface, 3, context);
    }
    contrast(roles['primary-foreground'], roles.primary, 'primary-foreground', 'primary', 4.5, context);
    cssVars[appearance] = roles;
  }
  return { cssVars };
}

function generate(check) {
  const tokens = parseJson(readFileSync(new URL('tokens.json', import.meta.url), 'utf8'));
  validate(tokens);
  const outputs = {
    'tailwind.generated.json': {
      appearance: tokens.defaults.appearance,
      theme: `design/theme-${tokens.defaults.accent}.generated.json`,
      tailwind: {
        spacing: tokens.space.step,
        fontSize: Object.fromEntries(sizes.map((key) => [key, tokens.type.scale[key]])),
        radius: { card: tokens.shape.card },
        breakpoints: { split: tokens.size.split, wide: tokens.size.wide },
      },
    },
  };
  for (const preset of presets) outputs[`theme-${preset}.generated.json`] = projectTheme(tokens, preset);
  const packageConfig = parseJson(readFileSync(new URL('../nsh.json', import.meta.url), 'utf8'));
  outputs['../nsh.json'] = { ...packageConfig, ...outputs['tailwind.generated.json'] };

  const stale = [];
  for (const [name, projection] of Object.entries(outputs)) {
    const output = new URL(name, import.meta.url);
    const expected = Buffer.from(`${JSON.stringify(projection, null, 2)}\n`);
    try {
      if (readFileSync(output).equals(expected)) continue;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    if (check) stale.push(name);
    else writeFileSync(output, expected);
  }
  if (stale.length) throw new Error(`${stale.join('、')} 缺失或过期。去掉 --check 后重新运行生成命令。`);
}

try {
  const args = process.argv.slice(2);
  requireValue(args.length === 0 || (args.length === 1 && args[0] === '--check'), '用法', 'node generate-tokens.mjs [--check]');
  generate(args.length === 1);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
