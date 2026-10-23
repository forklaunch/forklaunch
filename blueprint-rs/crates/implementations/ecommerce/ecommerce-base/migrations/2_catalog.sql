-- Stable indexed catalog pages stay bounded before DTO/variant/stock reads.
CREATE INDEX products_by_created_at ON fl_ecommerce.records
    (tenant_id,(data->>'createdAt'),id) WHERE kind='product';
