import { useQuery } from '@tanstack/react-query';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import { taxonomy } from '../data/mockData';
import { queryKeys } from './queryKeys';
import { useAdminApi } from './useAdminApi';

export type TaxonomySubcategoryRow = {
  id: string;
  categoryId: string;
  name: string;
  defaultAction: string;
};

export type TaxonomyCategoryRow = {
  id: string;
  name: string;
  defaultAction: string;
  subcategories: TaxonomySubcategoryRow[];
};

type TaxonomyState = {
  data: TaxonomyCategoryRow[];
  loading: boolean;
  error?: string;
  isMock: boolean;
};

type ApiCategory = {
  id: string;
  name: string;
  default_action: string;
};

type ApiSubcategory = {
  id: string;
  category_id: string;
  name: string;
  default_action: string;
};

const fallbackData: TaxonomyCategoryRow[] = taxonomy.categories.map((category) => ({
  id: category.id,
  name: category.name,
  defaultAction: category.defaultAction,
  subcategories: category.subcategories.map((sub) => ({
    id: sub.id,
    categoryId: category.id,
    name: sub.name,
    defaultAction: sub.defaultAction,
  })),
}));

const mapTaxonomy = (
  categories: ApiCategory[],
  subcategories: ApiSubcategory[],
): TaxonomyCategoryRow[] => {
  const byCategory = new Map<string, TaxonomySubcategoryRow[]>();
  subcategories.forEach((sub) => {
    const row: TaxonomySubcategoryRow = {
      id: sub.id,
      categoryId: sub.category_id,
      name: sub.name,
      defaultAction: sub.default_action,
    };
    const existing = byCategory.get(sub.category_id) ?? [];
    existing.push(row);
    byCategory.set(sub.category_id, existing);
  });

  return categories.map((category) => ({
    id: category.id,
    name: category.name,
    defaultAction: category.default_action,
    subcategories: byCategory.get(category.id) ?? [],
  }));
};

export const useTaxonomyData = () => {
  const { baseUrl, canCallApi, headers } = useAdminApi();
  const enabled = Boolean(baseUrl && canCallApi);

  const query = useQuery({
    queryKey: queryKeys.taxonomy(baseUrl),
    enabled,
    queryFn: async () => {
      const [categories, subcategories] = await Promise.all([
        adminGetJson<ApiCategory[]>(
          { baseUrl, canCallApi, headers } as AdminApiContext,
          '/api/v1/taxonomy/categories',
        ),
        adminGetJson<ApiSubcategory[]>(
          { baseUrl, canCallApi, headers } as AdminApiContext,
          '/api/v1/taxonomy/subcategories',
        ),
      ]);
      return mapTaxonomy(categories, subcategories);
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
