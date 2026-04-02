import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PendingClassificationsPage } from './PendingClassificationsPage';
import { usePendingClassifications } from '../hooks/usePendingClassifications';
import { usePendingActions } from '../hooks/usePendingActions';
import { useTaxonomyData, type TaxonomyActivationState } from '../hooks/useTaxonomyData';

vi.mock('../hooks/usePendingClassifications');
vi.mock('../hooks/usePendingActions');
vi.mock('../hooks/useTaxonomyData');

const mockedUsePendingClassifications = vi.mocked(usePendingClassifications);
const mockedUsePendingActions = vi.mocked(usePendingActions);
const mockedUseTaxonomyData = vi.mocked(useTaxonomyData);

const taxonomyState: TaxonomyActivationState = {
  version: 'canon-v1',
  categories: [
    {
      id: 'news-general',
      name: 'News',
      enabled: true,
      locked: false,
      subcategories: [
        { id: 'general-news', name: 'General News', enabled: true, locked: false },
        { id: 'local-news', name: 'Local News', enabled: true, locked: false },
      ],
    },
    {
      id: 'social-media',
      name: 'Social Media',
      enabled: true,
      locked: false,
      subcategories: [{ id: 'social-networking', name: 'Social Networking', enabled: true, locked: false }],
    },
  ],
};

describe('PendingClassificationsPage', () => {
  const refresh = vi.fn().mockResolvedValue(undefined);
  const manualClassify = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    mockedUsePendingClassifications.mockReturnValue({
      data: [
        {
          normalizedKey: 'domain:test.example',
          status: 'waiting_content',
          baseUrl: 'https://test.example',
          requestedAt: '2026-04-02T00:00:00Z',
          updatedAt: '2026-04-02T00:01:00Z',
        },
      ],
      loading: false,
      error: undefined,
      isMock: false,
      refresh,
      canCallApi: true,
      baseUrl: 'http://localhost:19000',
      headers: {},
    });
    mockedUsePendingActions.mockReturnValue({
      manualClassify,
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

  it('submits selected category and subcategory for manual classification', async () => {
    const user = userEvent.setup();
    mockedUseTaxonomyData.mockReturnValue({
      data: {
        ...taxonomyState,
        categories: taxonomyState.categories.map((category) =>
          category.id === 'social-media' ? { ...category, enabled: false } : category,
        ),
      },
      loading: false,
      error: undefined,
      isMock: false,
      refresh: vi.fn(),
      canCallApi: true,
    });
    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Manual Classify' }));
    });

    expect(screen.getByRole('option', { name: 'Social Media (disabled)' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Action')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Risk')).not.toBeInTheDocument();

    await act(async () => {
      await user.selectOptions(screen.getByLabelText('Category'), 'social-media');
    });

    const subcategorySelect = screen.getByLabelText('Subcategory') as HTMLSelectElement;
    expect(subcategorySelect.value).toBe('social-networking');

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Save Classification' }));
    });

    expect(manualClassify).toHaveBeenCalledWith(
      'domain:test.example',
      expect.objectContaining({
        primary_category: 'social-media',
        subcategory: 'social-networking',
      }),
    );
  });

  it('disables manual classify apply when taxonomy is unavailable', async () => {
    const user = userEvent.setup();
    mockedUseTaxonomyData.mockReturnValue({
      data: { ...taxonomyState, categories: [] },
      loading: false,
      error: 'offline',
      isMock: true,
      refresh: vi.fn(),
      canCallApi: false,
    });

    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Manual Classify' }));
    });

    expect(screen.getByRole('button', { name: 'Save Classification' })).toBeDisabled();
    expect(screen.getByText(/Taxonomy is unavailable for manual classification/i)).toBeInTheDocument();
  });
});
