import { expect } from '@wdio/globals';

// Helper to get text from option elements within a select
async function getOptionTexts(selectElement: ChainablePromiseElement | WebdriverIO.Element): Promise<string[]> {
  const element = await selectElement;
  const options = await element.$$('option');
  const texts: string[] = [];
  for (const opt of options) {
    texts.push(await opt.getText());
  }
  return texts;
}

type ChainablePromiseElement = ReturnType<typeof $>;

describe('Settings Page', () => {
  beforeEach(async () => {
    // Navigate to settings page
    const settingsLink = await $('a[href="/settings"]');
    await settingsLink.click();
    await browser.pause(500);
  });

  describe('Page Structure', () => {
    it('should display the settings header', async () => {
      const header = await $('h1');
      await expect(header).toHaveText('Settings');
    });

    it('should display the save button', async () => {
      const saveButton = await $('button*=Save');
      await expect(saveButton).toBeDisplayed();
    });
  });

  describe('Provider Settings Section', () => {
    it('should display LLM Provider section', async () => {
      const sectionHeader = await $('h2*=LLM Provider');
      await expect(sectionHeader).toBeDisplayed();
    });

    it('should have a provider dropdown', async () => {
      const providerSelect = await $('select');
      await expect(providerSelect).toBeDisplayed();
    });

    it('should display provider options', async () => {
      const providerSelect = await $$('select')[0];

      // Check some expected providers
      const optionTexts = await getOptionTexts(providerSelect);
      expect(optionTexts.length).toBeGreaterThan(0);
      expect(optionTexts).toContain('Anthropic (Claude)');
      expect(optionTexts).toContain('OpenAI (GPT)');
    });

    it('should have an API key input field', async () => {
      const apiKeyInput = await $('input[type="password"]');
      await expect(apiKeyInput).toBeDisplayed();
    });

    it('should have a model input field', async () => {
      // Find the model input (text input in provider section)
      const modelInputs = await $$('input[type="text"]');
      expect(modelInputs.length).toBeGreaterThan(0);
    });
  });

  describe('Approval Settings Section', () => {
    it('should display Approval Policy section', async () => {
      const sectionHeader = await $('h2*=Approval Policy');
      await expect(sectionHeader).toBeDisplayed();
    });

    it('should have auto-approve level dropdown', async () => {
      // This is the second select on the page
      const selects = await $$('select');
      expect(selects.length).toBeGreaterThanOrEqual(2);

      // Check auto-approve options exist
      const autoApproveSelect = selects[1];
      const optionTexts = await getOptionTexts(autoApproveSelect);
      expect(optionTexts).toContain('None (Ask for everything)');
    });

    it('should have confirmation dialogs checkbox', async () => {
      const checkbox = await $('input[type="checkbox"]');
      await expect(checkbox).toBeDisplayed();
    });
  });

  describe('UI Settings Section', () => {
    it('should display User Interface section', async () => {
      const sectionHeader = await $('h2*=User Interface');
      await expect(sectionHeader).toBeDisplayed();
    });

    it('should have theme dropdown', async () => {
      const selects = await $$('select');
      expect(selects.length).toBeGreaterThanOrEqual(3);

      // Theme select should have system, light, dark options
      const themeSelect = selects[2];
      const optionTexts = await getOptionTexts(themeSelect);
      expect(optionTexts).toContain('System');
      expect(optionTexts).toContain('Light');
      expect(optionTexts).toContain('Dark');
    });
  });

  describe('Form Interactions', () => {
    it('should allow changing the provider', async () => {
      const providerSelect = await $$('select')[0];
      await providerSelect.selectByVisibleText('OpenAI (GPT)');

      const selectedValue = await providerSelect.getValue();
      expect(selectedValue).toBe('openai');
    });

    it('should allow typing in the model field', async () => {
      const modelInput = await $$('input[type="text"]')[0];
      await modelInput.clearValue();
      await modelInput.setValue('gpt-4');

      const value = await modelInput.getValue();
      expect(value).toBe('gpt-4');
    });

    it('should allow toggling checkboxes', async () => {
      const checkbox = await $$('input[type="checkbox"]')[0];
      const initialState = await checkbox.isSelected();

      await checkbox.click();
      const newState = await checkbox.isSelected();

      expect(newState).not.toBe(initialState);

      // Click again to restore
      await checkbox.click();
    });
  });
});
