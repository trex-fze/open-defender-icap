import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PendingClassificationsPage } from './PendingClassificationsPage';
import { useLlmProviders } from '../hooks/useLlmProviders';
import { usePendingClassifications } from '../hooks/usePendingClassifications';
import { usePendingActions } from '../hooks/usePendingActions';
import { useTaxonomyData, type TaxonomyActivationState } from '../hooks/useTaxonomyData';

vi.mock('../hooks/useLlmProviders');
vi.mock('../hooks/usePendingClassifications');
vi.mock('../hooks/usePendingActions');
vi.mock('../hooks/useTaxonomyData');

const mockedUseLlmProviders = vi.mocked(useLlmProviders);
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
  const metadataClassify = vi.fn().mockResolvedValue(undefined);
  const clearPending = vi.fn().mockResolvedValue(undefined);
  const clearAllPending = vi.fn().mockResolvedValue(1);

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
      metadataClassify,
      clearPending,
      clearAllPending,
      busyKey: undefined,
      busyAll: false,
      error: undefined,
      canCallApi: true,
    });
    mockedUseLlmProviders.mockReturnValue({
      data: [
        {
          name: 'local-lmstudio',
          providerType: 'lmstudio',
          role: 'primary',
          healthStatus: 'healthy',
        },
        {
          name: 'openai-fallback',
          providerType: 'openai',
          role: 'fallback',
          healthStatus: 'healthy',
        },
      ],
      loading: false,
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

  it('deletes a single pending site record', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Delete' }));
    });

    expect(clearPending).toHaveBeenCalledWith('domain:test.example');
    expect(refresh).toHaveBeenCalled();
    expect(screen.getByText(/Deleted pending site domain:test.example/i)).toBeInTheDocument();
  });

  it('requires exact guard phrase for delete all', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'prompt').mockReturnValue('delete all');

    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Delete All Pending' }));
    });

    expect(clearAllPending).not.toHaveBeenCalled();
    expect(screen.getByText(/confirmation phrase mismatch/i)).toBeInTheDocument();
  });

  it('deletes all pending records when guard phrase matches', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'prompt').mockReturnValue('DELETE ALL');
    clearAllPending.mockResolvedValueOnce(7);

    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Delete All Pending' }));
    });

    expect(clearAllPending).toHaveBeenCalled();
    expect(refresh).toHaveBeenCalled();
    expect(screen.getByText(/Deleted 7 pending sites/i)).toBeInTheDocument();
  });

  it('queues metadata-only classification with preferred provider', async () => {
    const user = userEvent.setup();

    render(<PendingClassificationsPage />);

    await act(async () => {
      await user.click(screen.getByRole('button', { name: 'Metadata-only Classify' }));
    });

    await act(async () => {
      await user.selectOptions(screen.getByLabelText('Preferred Provider'), 'openai-fallback');
      await user.click(screen.getByRole('button', { name: 'Queue Metadata-only Classification' }));
    });

    expect(metadataClassify).toHaveBeenCalledWith(
      'domain:test.example',
      expect.objectContaining({ provider_name: 'openai-fallback' }),
    );
    expect(refresh).toHaveBeenCalled();
    expect(screen.getByText(/Queued metadata-only classification/i)).toBeInTheDocument();
  });
});
