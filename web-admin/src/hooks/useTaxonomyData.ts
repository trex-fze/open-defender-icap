import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { taxonomyActivation } from '../data/mockData';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

type ApiTaxonomyResponse = {
  version: string;
  updated_at?: string;
  updated_by?: string;
  categories: ApiTaxonomyCategory[];
};

type ApiTaxonomyCategory = {
  id: string;
  name: string;
  enabled: boolean;
  locked: boolean;
  subcategories: ApiTaxonomySubcategory[];
};

type ApiTaxonomySubcategory = {
  id: string;
  name: string;
  enabled: boolean;
  locked: boolean;
};

export type TaxonomySubcategoryRow = {
  id: string;
  name: string;
  enabled: boolean;
  locked: boolean;
};

export type TaxonomyCategoryRow = {
  id: string;
  name: string;
  enabled: boolean;
  locked: boolean;
  subcategories: TaxonomySubcategoryRow[];
};

export type TaxonomyActivationState = {
  version: string;
  updatedAt?: string;
  updatedBy?: string;
  categories: TaxonomyCategoryRow[];
};

type TaxonomyState = {
  data: TaxonomyActivationState;
  loading: boolean;
  error?: string;
  isMock: boolean;
};

const mapResponse = (payload: ApiTaxonomyResponse): TaxonomyActivationState => ({
  version: payload.version,
  updatedAt: payload.updated_at,
  updatedBy: payload.updated_by,
  categories: payload.categories.map((category) => ({
    id: category.id,
    name: category.name,
    enabled: category.enabled,
    locked: category.locked,
    subcategories: category.subcategories.map((sub) => ({
      id: sub.id,
      name: sub.name,
      enabled: sub.enabled,
      locked: sub.locked,
    })),
  })),
});

const fallbackData: TaxonomyActivationState = {
  version: taxonomyActivation.version,
  updatedAt: taxonomyActivation.updatedAt,
  updatedBy: taxonomyActivation.updatedBy,
  categories: taxonomyActivation.categories.map((category) => ({
    id: category.id,
    name: category.name,
    enabled: category.enabled,
    locked: category.locked,
    subcategories: category.subcategories.map((sub) => ({
      id: sub.id,
      name: sub.name,
      enabled: sub.enabled,
      locked: sub.locked,
    })),
  })),
};

export const useTaxonomyData = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.taxonomy(baseUrl),
    enabled,
    queryFn: async () => {
      const payload = await adminGetJson<ApiTaxonomyResponse>(
        { baseUrl, canCallApi, headers } as AdminApiContext,
        '/api/v1/taxonomy',
      );
      return mapResponse(payload);
    },
  });

  const refresh = async () => {
    await query.refetch();
  };

  const state: TaxonomyState = !enabled
    ? { data: fallbackData, loading: false, isMock: true }
    : query.isError
      ? {
          data: fallbackData,
          loading: false,
          error: query.error instanceof Error ? query.error.message : 'Failed to fetch taxonomy',
          isMock: true,
        }
      : {
          data: query.data ?? fallbackData,
          loading: query.isLoading,
          isMock: false,
        };

  return {
    ...state,
    refresh,
    canCallApi,
  } as const;
};
