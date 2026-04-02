import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ClassificationsPage } from './ClassificationsPage';
import { useClassificationsData } from '../hooks/useClassificationsData';
import { useClassificationActions } from '../hooks/useClassificationActions';
import { useTaxonomyData, type TaxonomyActivationState } from '../hooks/useTaxonomyData';

vi.mock('../hooks/useClassificationsData');
vi.mock('../hooks/useClassificationActions');
vi.mock('../hooks/useTaxonomyData');

const mockedUseClassificationsData = vi.mocked(useClassificationsData);
const mockedUseClassificationActions = vi.mocked(useClassificationActions);
const mockedUseTaxonomyData = vi.mocked(useTaxonomyData);

const taxonomyState: TaxonomyActivationState = {
  version: 'canon-v1',
  categories: [
    {
      id: 'social-media',
      name: 'Social Media',
      enabled: true,
      locked: false,
      subcategories: [{ id: 'social-networking', name: 'Social Networking', enabled: true, locked: false }],
    },
    {
      id: 'news-general',
      name: 'News',
      enabled: true,
      locked: false,
      subcategories: [{ id: 'general-news', name: 'General News', enabled: true, locked: false }],
    },
  ],
};

describe('ClassificationsPage', () => {
  const refresh = vi.fn().mockResolvedValue(undefined);
  const updateClassification = vi.fn().mockResolvedValue(undefined);
  const deleteClassification = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('confirm', vi.fn(() => true));
    mockedUseClassificationsData.mockReturnValue({
      data: [
        {
          normalized_key: 'domain:tiktok.com',
          state: 'classified',
          primary_category: 'social-media',
          subcategory: 'social-networking',
          recommended_action: 'Monitor',
          status: 'active',
          updated_at: '2026-04-02T00:01:00Z',
        },
      ],
      loading: false,
      error: undefined,
      refresh,
      canCallApi: true,
      isMock: false,
    });
    mockedUseClassificationActions.mockReturnValue({
      updateClassification,
      deleteClassification,
      busyKey: undefined,
      error: undefined,
      canCallApi: true,
    });
    mockedUseTaxonomyData.mockReturnValue({
      data: taxonomyState,
      loading: false,
      error: undefined,
      isMock: false,
      refresh: vi.fn(),
      canCallApi: true,
    });
  });

  it('prefills existing taxonomy selection and saves updates', async () => {
    const user = userEvent.setup();
    render(<ClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Edit' }));
    });

    expect((screen.getByLabelText('Category') as HTMLSelectElement).value).toBe('social-media');
    expect((screen.getByLabelText('Subcategory') as HTMLSelectElement).value).toBe('social-networking');

    await act(async () => {
      await user.selectOptions(screen.getByLabelText('Category'), 'news-general');
      await user.click(screen.getByRole('button', { name: 'Save' }));
    });

    expect(updateClassification).toHaveBeenCalledWith(
      'domain:tiktok.com',
      expect.objectContaining({
        primary_category: 'news-general',
        subcategory: 'general-news',
      }),
    );
  });

  it('removes a key from classifications list', async () => {
    const user = userEvent.setup();
    render(<ClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Edit' }));
    });

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Remove Domain' }));
    });

    expect(deleteClassification).toHaveBeenCalledWith('domain:tiktok.com');
  });
});
