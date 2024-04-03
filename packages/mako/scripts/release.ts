import assert from 'assert';
import 'zx/globals';

(async () => {
  if (argv.build) {
    await build();
  } else {
    await run();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});

async function run() {
  // check git status
  console.log('Check git status');
  const status = (await $`git status --porcelain`).stdout.trim();
  if (status) {
    throw new Error('Please commit all changes before release');
  }

  // check git remote update
  console.log('check git remote update');
  await $`git fetch`;
  const gitStatus = (await $`git status --short --branch`).stdout.trim();
  assert(!gitStatus.includes('behind'), `git status is behind remote`);

  // check docker status
  console.log('Check docker status');
  await $`docker ps`;

  // bump version
  console.log('Bump version');
  const nodePkgDir = path.resolve(__dirname, '..');
  const nodePkgPath = path.join(nodePkgDir, 'package.json');
  const nodePkg = JSON.parse(fs.readFileSync(nodePkgPath, 'utf-8'));
  const currentVersion = nodePkg.version;

  console.log('current version: ', currentVersion);
  const newVersion = (await question(`What's next version? `)).trim();

  let tag = 'latest';
  if (
    newVersion.includes('-alpha.') ||
    newVersion.includes('-beta.') ||
    newVersion.includes('-rc.')
  )
    tag = 'next';
  if (newVersion.includes('-canary.')) tag = 'canary';
  if (newVersion.includes('-dev.')) tag = 'dev';

  console.log('Check branch');
  const branch = (await $`git branch --show-current`).stdout.trim();
  if (tag === 'latest') {
    if (branch !== 'master') {
      throw new Error('publishing latest tag needs to be in master branch');
    }
  }

  // confirm
  console.log(`${nodePkg.name}@${newVersion} will be published`);
  const willContinue = ((await question('Continue? y/[n]')) || 'n').trim();
  if (willContinue !== 'y') {
    console.log('Abort!');
    process.exit(0);
  }

  // update version to package.json
  nodePkg.version = newVersion;
  fs.writeFileSync(nodePkgPath, JSON.stringify(nodePkg, null, 2) + '\n');

  // build
  await build();

  // publish
  await $`npm publish --tag ${tag} --access public`;

  // set new version to bundler-okam
  console.log('Set new version to bundler-okam');
  const bundlerOkamPkgPath = path.join(
    __dirname,
    '../../../packages/bundler-okam/package.json',
  );
  const bundlerOkamPkg = JSON.parse(
    fs.readFileSync(bundlerOkamPkgPath, 'utf-8'),
  );
  bundlerOkamPkg.dependencies['@okamjs/okam'] = `${newVersion}`;
  fs.writeFileSync(
    bundlerOkamPkgPath,
    JSON.stringify(bundlerOkamPkg, null, 2) + '\n',
  );

  await $`git commit -an -m "release: ${nodePkg.name}@${newVersion}"`;
  // tag
  console.log('Tag');
  await $`git tag v${newVersion}`;

  // push
  console.log('Push');
  await $`git push origin ${branch} --tags`;
}

async function build() {
  // clean
  await $`rm -rf ./*.node`;
  await $`find ./npm -name '*.node' | xargs rm -f`;
  await $`rm -rf ./dist`;

  // build linux *.node
  console.log('linux building started...');
  const start = Date.now();
  const cargoRoot = path.join(__dirname, '../../..');
  // clean sailfish
  // since it's lock files may cause build error
  await $`rm -rf ${cargoRoot}/target/release/build/sailfish*`;
  await build_linux_binding();
  await $`pnpm run format`;
  const duration = (Date.now() - start) / 1000;
  console.log(`linux building done ${duration}s`);

  // build macos *.node
  await $`cargo build --lib -r --target x86_64-apple-darwin`;
  await $`pnpm run build:mac:x86`;
  await $`cargo build --lib -r  --target aarch64-apple-darwin`;
  await $`pnpm run build:mac:aarch`;
  await $`strip -x ./okam.darwin-*.node`;

  // build src
  await $`pnpm run src:build`;

  // move artifacts to npm
  await $`pnpm run artifacts:local`;
}

async function build_linux_binding() {
  const isArm = process.arch === 'arm64';
  const cargoBase = path.join(
    process.env['CARGO_HOME'] || process.env['HOME']!,
    '.cargo',
  );
  const cargoMapOption = (p) => [
    '-v',
    `${path.join(cargoBase, p)}:${path.join('/usr/local/cargo', p)}`,
  ];
  const rustupRoot = path.join(os.homedir(), '.rustup');
  const makoRoot = path.join(__dirname, '../../..');
  const volumeOptions = [
    ...cargoMapOption('config'),
    ...cargoMapOption('git/db'),
    ...cargoMapOption('registry/cache'),
    ...cargoMapOption('registry/index'),
    ...[`-v`, `${makoRoot}:/build`],
    ...[`-v`, `${rustupRoot}:/usr/local/rustup`],
    ...[`-w`, `/build`],
  ];
  const containerCMD = [
    'cargo build -r --lib --target x86_64-unknown-linux-gnu',
    'cd packages/mako',
    'npm run build:linux:x86',
    'strip okam.linux*.node',
  ].join('&&');
  const envOptions: string[] = [];
  if (process.env['RUSTUP_DIST_SERVER']) {
    envOptions.push(
      ...['-e', `RUSTUP_DIST_SERVER=${process.env['RUSTUP_DIST_SERVER']}`],
    );
  }
  if (process.env[`RUSTUP_UPDATE_ROOT`]) {
    envOptions.push(
      ...['-e', `RUSTUP_UPDATE_ROOT=${process.env[`RUSTUP_UPDATE_ROOT`]}`],
    );
  }
  const options = ['--rm', ...volumeOptions, ...envOptions];
  if (isArm) {
    options.push(...['--platform', 'linux/amd64']);
  }
  const image = 'ghcr.io/napi-rs/napi-rs/nodejs-rust:lts-debian';
  await $`docker run ${options} ${image} bash -c ${containerCMD}`;
}
