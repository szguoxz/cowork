import * as path from 'path';
import { spawn, ChildProcess } from 'child_process';

// Path to the built Tauri application
const tauriAppPath = process.env.TAURI_APP_PATH || path.resolve(
  __dirname,
  '../target/release/cowork-app'
);

let tauriDriver: ChildProcess | null = null;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const config: any = {
  // Use WebDriver protocol (tauri-driver implements WebDriver)
  runner: 'local',

  autoCompileOpts: {
    autoCompile: true,
    tsNodeOpts: {
      project: './tsconfig.json',
      transpileOnly: true,
    },
  },

  specs: ['./e2e/**/*.spec.ts'],
  exclude: [],

  maxInstances: 1, // Tauri apps should run one at a time

  capabilities: [
    {
      // Use tauri-driver as the browser
      browserName: 'wry',
      'tauri:options': {
        application: tauriAppPath,
      },
    },
  ],

  logLevel: 'info',
  bail: 0,
  baseUrl: '',
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,

  framework: 'mocha',
  reporters: ['spec'],

  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  // Start tauri-driver before tests
  onPrepare: async function () {
    // Ensure the app is built
    console.log('Starting tauri-driver on port 4444...');
    tauriDriver = spawn('tauri-driver', ['--port', '4444'], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    tauriDriver.stdout?.on('data', (data) => {
      console.log(`tauri-driver: ${data}`);
    });

    tauriDriver.stderr?.on('data', (data) => {
      console.error(`tauri-driver error: ${data}`);
    });

    // Wait for tauri-driver to be ready
    await new Promise((resolve) => setTimeout(resolve, 2000));
    console.log('tauri-driver started');
  },

  // Stop tauri-driver after tests
  onComplete: async function () {
    console.log('Stopping tauri-driver...');
    if (tauriDriver) {
      tauriDriver.kill();
      tauriDriver = null;
    }
  },

  // WebDriver server config (tauri-driver)
  hostname: 'localhost',
  port: 4444,
  path: '/',
};
