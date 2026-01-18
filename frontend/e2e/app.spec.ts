import { expect } from '@wdio/globals';

describe('Cowork App', () => {
  describe('Application Launch', () => {
    it('should launch the application', async () => {
      // Wait for the app to load
      await browser.waitUntil(
        async () => {
          const title = await browser.getTitle();
          return title.length > 0;
        },
        { timeout: 10000, timeoutMsg: 'App did not load' }
      );

      const title = await browser.getTitle();
      expect(title).toBe('Cowork');
    });

    it('should display the sidebar', async () => {
      const sidebar = await $('aside');
      await expect(sidebar).toBeDisplayed();
    });

    it('should display the main content area', async () => {
      const main = await $('main');
      await expect(main).toBeDisplayed();
    });
  });

  describe('Navigation', () => {
    it('should start on the Chat page by default', async () => {
      // The Chat page should be active by default
      const chatLink = await $('a[href="/"]');
      await expect(chatLink).toBeDisplayed();
    });

    it('should navigate to Files page', async () => {
      const filesLink = await $('a[href="/files"]');
      await filesLink.click();

      // Wait for navigation
      await browser.pause(500);

      // Verify we're on the files page
      const url = await browser.getUrl();
      expect(url).toContain('/files');
    });

    it('should navigate to Settings page', async () => {
      const settingsLink = await $('a[href="/settings"]');
      await settingsLink.click();

      // Wait for navigation
      await browser.pause(500);

      // Verify we're on the settings page
      const url = await browser.getUrl();
      expect(url).toContain('/settings');
    });

    it('should navigate back to Chat page', async () => {
      const chatLink = await $('a[href="/"]');
      await chatLink.click();

      // Wait for navigation
      await browser.pause(500);

      // Verify we're back on the chat page
      const url = await browser.getUrl();
      // Home page URL ends with / or just the host
      expect(url).not.toContain('/files');
      expect(url).not.toContain('/settings');
    });
  });

  describe('Accessibility', () => {
    it('should have navigation links with titles', async () => {
      const chatLink = await $('a[title="Chat"]');
      const filesLink = await $('a[title="Files"]');
      const settingsLink = await $('a[title="Settings"]');

      await expect(chatLink).toExist();
      await expect(filesLink).toExist();
      await expect(settingsLink).toExist();
    });
  });
});
