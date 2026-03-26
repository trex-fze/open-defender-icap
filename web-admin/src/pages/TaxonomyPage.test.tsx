import { render, screen, cleanup, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TaxonomyPage } from './TaxonomyPage';
import { useTaxonomyData, type TaxonomyActivationState } from '../hooks/useTaxonomyData';
import { useTaxonomyActions } from '../hooks/useTaxonomyActions';
import type { ActivationUpdatePayload } from '../hooks/useTaxonomyActions';

vi.mock('../hooks/useTaxonomyData');
vi.mock('../hooks/useTaxonomyActions');

const mockedUseTaxonomyData = vi.mocked(useTaxonomyData);
const mockedUseTaxonomyActions = vi.mocked(useTaxonomyActions);

const buildActivationState = (): TaxonomyActivationState => ({
  version: 'canon-v1',
  updatedAt: '2026-03-24T00:00:00Z',
  updatedBy: 'ops-duty',
  categories: [
    {
      id: 'unknown-unclassified',
      name: 'Unknown / Unclassified',
      enabled: true,
      locked: false,
      subcategories: [
        { id: 'newly-seen-unknowns', name: 'Newly seen unknowns', enabled: true, locked: false },
        { id: 'insufficient-evidence', name: 'Insufficient evidence', enabled: true, locked: false },
      ],
    },
    {
      id: 'critical-infrastructure',
      name: 'Critical Infrastructure',
      enabled: true,
      locked: true,
      subcategories: [
        { id: 'root-dns', name: 'Root DNS', enabled: true, locked: true },
        { id: 'public-safety-systems', name: 'Public safety systems', enabled: true, locked: false },
      ],
    },
    {
      id: 'news-general',
      name: 'News',
      enabled: true,
      locked: false,
      subcategories: [{ id: 'general-news', name: 'General News', enabled: true, locked: false }],
    },
  ],
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe('TaxonomyPage', () => {
  it('renders locked taxonomy entries as read-only', async () => {
    const refresh = vi.fn().mockResolvedValue(undefined);
    mockedUseTaxonomyData.mockReturnValue({
      data: buildActivationState(),
      loading: false,
      error: undefined,
      isMock: false,
      refresh,
      canCallApi: true,
    });
    mockedUseTaxonomyActions.mockReturnValue({
      saveActivation: vi.fn().mockResolvedValue(undefined),
      busy: false,
      error: undefined,
      canCallApi: true,
    });

    render(<TaxonomyPage />);
    await screen.findByLabelText('Unknown / Unclassified');

    const lockedCategory = screen.getByLabelText('Critical Infrastructure') as HTMLInputElement;
    const lockedSubcategory = screen.getByLabelText(/Root DNS/) as HTMLInputElement;

    expect(lockedCategory).toBeDisabled();
    expect(lockedSubcategory).toBeDisabled();
    expect(screen.getAllByText('Locked').length).toBeGreaterThan(0);
  });

  it('allows operators to configure subcategories independently of category toggle', async () => {
    const refresh = vi.fn().mockResolvedValue(undefined);
    const saveActivation = vi.fn().mockResolvedValue(undefined);
    mockedUseTaxonomyData.mockReturnValue({
      data: buildActivationState(),
      loading: false,
      error: undefined,
      isMock: false,
      refresh,
      canCallApi: true,
    });
    mockedUseTaxonomyActions.mockReturnValue({
      saveActivation,
      busy: false,
      error: undefined,
      canCallApi: true,
    });

    const user = userEvent.setup();
    render(<TaxonomyPage />);
    await screen.findByLabelText('Unknown / Unclassified');

    const unknownCategory = screen.getByLabelText('Unknown / Unclassified') as HTMLInputElement;
    const unknownSubcategory = screen.getByLabelText('Newly seen unknowns') as HTMLInputElement;

    expect(unknownCategory).not.toBeChecked();

    // Disable category -> sub toggles remain editable for future configuration
    await act(async () => {
      await user.click(unknownCategory);
    });

    expect(unknownCategory).toBeChecked();
    expect(unknownSubcategory).not.toBeChecked();
    expect(unknownSubcategory).not.toBeDisabled();

    // Disable a specific subcategory while parent is disabled
    await act(async () => {
      await user.click(screen.getByLabelText('Newly seen unknowns'));
    });
    expect(screen.getByLabelText('Newly seen unknowns')).toBeChecked();

    // Re-enable category -> subs keep configured choices
    await act(async () => {
      await user.click(unknownCategory);
    });

    expect(unknownCategory).not.toBeChecked();
    expect(screen.getByLabelText('Newly seen unknowns')).toBeChecked();

    const saveButton = screen.getByRole('button', { name: /Save Changes/i });
    await act(async () => {
      await user.click(saveButton);
    });
    await screen.findByText('Activation profile saved');

    expect(saveActivation).toHaveBeenCalledTimes(1);
    const payload = saveActivation.mock.calls[0][0] as ActivationUpdatePayload;
    const unknownPayload = payload.categories.find((cat: { id: string }) => cat.id === 'unknown-unclassified');
    expect(unknownPayload?.enabled).toBe(true);
    unknownPayload?.subcategories?.forEach((sub: { id: string; enabled: boolean }) => {
      if (sub.id === 'newly-seen-unknowns') {
        expect(sub.enabled).toBe(false);
      } else {
        expect(sub.enabled).toBe(true);
      }
    });
    expect(refresh).toHaveBeenCalledTimes(1);

    const resetButton = screen.getByRole('button', { name: 'Reset' });
    await act(async () => {
      await user.click(resetButton);
    });
    expect(screen.getByLabelText('Unknown / Unclassified')).not.toBeChecked();
  });

  it('shows mock banner and disables editing when Admin API is offline', async () => {
    const refresh = vi.fn().mockResolvedValue(undefined);
    mockedUseTaxonomyData.mockReturnValue({
      data: buildActivationState(),
      loading: false,
      error: 'offline',
      isMock: true,
      refresh,
      canCallApi: false,
    });
    mockedUseTaxonomyActions.mockReturnValue({
      saveActivation: vi.fn().mockResolvedValue(undefined),
      busy: false,
      error: undefined,
      canCallApi: false,
    });

    render(<TaxonomyPage />);
    await screen.findByLabelText('Unknown / Unclassified');

    expect(screen.getByText(/Mock stream/)).toBeInTheDocument();
    const saveButton = screen.getByRole('button', { name: /Save Changes/i });
    expect(saveButton).toBeDisabled();
    expect(screen.getByLabelText('Unknown / Unclassified')).toBeDisabled();
  });
});
