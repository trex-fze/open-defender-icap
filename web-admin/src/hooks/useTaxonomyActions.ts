import { useState } from 'react';
import {
  adminDelete,
  adminPostJson,
  adminPutJson,
  type AdminApiContext,
} from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

export type CategoryInput = {
  name: string;
  defaultAction: string;
};

export type SubcategoryInput = {
  categoryId: string;
  name: string;
  defaultAction: string;
};

type CategoryPayload = {
  name: string;
  default_action: string;
};

type SubcategoryPayload = {
  category_id: string;
  name: string;
  default_action: string;
};

const normalizeAction = (value: string) => value.trim().toLowerCase();

export const useTaxonomyActions = () => {
  const api = useAdminApi();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const createCategory = async (input: CategoryInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: CategoryPayload = {
        name: input.name.trim(),
        default_action: normalizeAction(input.defaultAction),
      };
      await adminPostJson<unknown, CategoryPayload>(
        api as AdminApiContext,
        '/api/v1/taxonomy/categories',
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create category';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const updateCategory = async (id: string, input: CategoryInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: CategoryPayload = {
        name: input.name.trim(),
        default_action: normalizeAction(input.defaultAction),
      };
      await adminPutJson<unknown, CategoryPayload>(
        api as AdminApiContext,
        `/api/v1/taxonomy/categories/${id}`,
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update category';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const deleteCategory = async (id: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/taxonomy/categories/${id}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete category';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const createSubcategory = async (input: SubcategoryInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: SubcategoryPayload = {
        category_id: input.categoryId,
        name: input.name.trim(),
        default_action: normalizeAction(input.defaultAction),
      };
      await adminPostJson<unknown, SubcategoryPayload>(
        api as AdminApiContext,
        '/api/v1/taxonomy/subcategories',
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to create subcategory';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const updateSubcategory = async (id: string, input: SubcategoryInput): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      const payload: SubcategoryPayload = {
        category_id: input.categoryId,
        name: input.name.trim(),
        default_action: normalizeAction(input.defaultAction),
      };
      await adminPutJson<unknown, SubcategoryPayload>(
        api as AdminApiContext,
        `/api/v1/taxonomy/subcategories/${id}`,
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update subcategory';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  const deleteSubcategory = async (id: string): Promise<void> => {
    setBusy(true);
    setError(undefined);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/taxonomy/subcategories/${id}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete subcategory';
      setError(message);
      throw err;
    } finally {
      setBusy(false);
    }
  };

  return {
    createCategory,
    updateCategory,
    deleteCategory,
    createSubcategory,
    updateSubcategory,
    deleteSubcategory,
    busy,
    error,
    canCallApi: api.canCallApi,
  };
};
