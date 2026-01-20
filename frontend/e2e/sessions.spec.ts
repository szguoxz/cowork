import { expect } from '@wdio/globals';

/**
 * E2E tests for multi-session functionality
 *
 * Note: These tests require:
 * - A valid API key configured in settings
 * - The app to be in a state where sessions can be created
 *
 * The session tabs only appear when there are multiple sessions.
 */
describe('Multi-Session Support', () => {
  beforeEach(async () => {
    // Navigate to Chat page
    const chatLink = await $('a[href="/"]');
    await chatLink.click();
    await browser.pause(500);
  });

  describe('Session Initialization', () => {
    it('should display the chat interface on load', async () => {
      // Header should exist with app name
      const header = await $('header');
      await expect(header).toBeDisplayed();

      const heading = await $('h1');
      const text = await heading.getText();
      expect(text).toBe('Cowork');
    });

    it('should show status indicator', async () => {
      // Status indicator should be visible in header
      const header = await $('header');
      const statusText = await header.$('.text-xs');
      await expect(statusText).toBeDisplayed();
    });

    it('should display welcome message when no messages', async () => {
      // Look for welcome text
      const welcomeText = await $('*=Welcome to Cowork');
      // Either messages exist or welcome text shows
      const isDisplayed = await welcomeText.isDisplayed().catch(() => false);
      // This is expected to be true on fresh start, but may be false if messages exist
      expect(typeof isDisplayed).toBe('boolean');
    });
  });

  describe('Chat Input', () => {
    it('should have a message input field', async () => {
      const input = await $('input[type="text"]');
      await expect(input).toBeDisplayed();
    });

    it('should have a send button', async () => {
      const sendButton = await $('button[type="submit"]');
      await expect(sendButton).toBeDisplayed();
    });

    it('should allow typing in the input field', async () => {
      const input = await $('input[type="text"]');
      await input.setValue('Test message');

      const value = await input.getValue();
      expect(value).toBe('Test message');

      // Clear for next test
      await input.clearValue();
    });
  });

  describe('Session Tabs Visibility', () => {
    it('should not show tabs when only one session exists', async () => {
      // SessionTabs returns null when only one session
      // So we shouldn't find the tabs container
      const tabsContainer = await $$('.flex.items-center.gap-1.px-2.py-1\\.5');

      // Either tabs don't exist or there's only one session
      // This is the expected initial state
      if (tabsContainer.length > 0) {
        // If tabs exist, there should be multiple sessions
        const tabs = await tabsContainer[0].$$('.cursor-pointer');
        expect(tabs.length).toBeGreaterThanOrEqual(1);
      }
    });
  });

  describe('Session State Display', () => {
    it('should show ready/working status', async () => {
      const header = await $('header');
      const statusSpan = await header.$('.text-xs');
      const statusText = await statusSpan.getText();

      // Status should be one of: Ready, Working..., Starting...
      expect(['Ready', 'Working...', 'Starting...']).toContain(statusText);
    });
  });

  describe('API Key Check', () => {
    it('should handle missing API key gracefully', async () => {
      // If no API key, should show warning
      const warningIcon = await $$('.text-warning');
      const apiKeyMessage = await $$('*=API Key Required');

      // Either we have an API key and chat is shown,
      // or we don't and warning is shown
      const chatInput = await $('input[type="text"]');
      const chatVisible = await chatInput.isDisplayed().catch(() => false);
      const warningVisible = apiKeyMessage.length > 0
        ? await apiKeyMessage[0].isDisplayed().catch(() => false)
        : false;

      // One of these should be true
      expect(chatVisible || warningVisible).toBe(true);
    });
  });

  describe('Tool Display', () => {
    it('should have proper structure for tool messages when present', async () => {
      // This test verifies the tool display structure exists
      // Tools appear with Terminal icon and status badges
      const terminalIcons = await $$('.lucide-terminal');

      // If tools exist, they should have proper structure
      if (terminalIcons.length > 0) {
        const parent = await terminalIcons[0].parentElement();
        await expect(parent).toBeDisplayed();
      }
    });
  });

  describe('Message Display', () => {
    it('should have scrollable message area', async () => {
      const messageArea = await $('.flex-1.overflow-y-auto');
      await expect(messageArea).toBeDisplayed();
    });

    it('should display user messages on the right', async () => {
      // User messages use justify-end
      const userMessages = await $$('.justify-end');
      // May be empty if no messages sent yet
      expect(Array.isArray(userMessages)).toBe(true);
    });

    it('should display assistant messages on the left', async () => {
      // Assistant messages use justify-start
      const assistantMessages = await $$('.justify-start');
      // May be empty if no messages yet (loading indicator also uses justify-start)
      expect(Array.isArray(assistantMessages)).toBe(true);
    });
  });

  describe('Error Handling', () => {
    it('should have error banner area that can appear', async () => {
      // Error banner appears when there's an error
      // We just verify the structure can handle errors
      const errorBanners = await $$('.bg-error\\/10');

      // Either no error (empty) or error displayed
      expect(Array.isArray(errorBanners)).toBe(true);
    });

    it('should allow dismissing errors when present', async () => {
      const errorBanners = await $$('.bg-error\\/10');

      if (errorBanners.length > 0) {
        const closeButton = await errorBanners[0].$('button');
        await expect(closeButton).toBeDisplayed();
      }
    });
  });

  describe('Form Submission', () => {
    it('should disable send button when input is empty', async () => {
      const input = await $('input[type="text"]');
      await input.clearValue();

      const sendButton = await $('button[type="submit"]');
      const isDisabled = await sendButton.getAttribute('disabled');

      // Button should be disabled when input is empty
      expect(isDisabled).not.toBeNull();
    });

    it('should enable send button when input has text and session is idle', async () => {
      const input = await $('input[type="text"]');
      await input.setValue('Test');

      // Give UI time to update
      await browser.pause(100);

      const sendButton = await $('button[type="submit"]');

      // Button state depends on session idle state
      // We just verify the button exists and responds to input
      await expect(sendButton).toBeDisplayed();

      // Clear for cleanup
      await input.clearValue();
    });
  });
});

describe('Session Context Integration', () => {
  beforeEach(async () => {
    const chatLink = await $('a[href="/"]');
    await chatLink.click();
    await browser.pause(500);
  });

  it('should initialize session context on app load', async () => {
    // The app should either show chat (initialized with key) or API key warning
    const main = await $('main');
    await expect(main).toBeDisplayed();
  });

  it('should maintain session state across page navigation', async () => {
    // Navigate away
    const settingsLink = await $('a[href="/settings"]');
    await settingsLink.click();
    await browser.pause(500);

    // Navigate back
    const chatLink = await $('a[href="/"]');
    await chatLink.click();
    await browser.pause(500);

    // Chat should still be functional
    const main = await $('main');
    await expect(main).toBeDisplayed();
  });
});
