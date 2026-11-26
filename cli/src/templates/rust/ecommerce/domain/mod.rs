//! Ecommerce domain types and services come from the interface and base
//! crates. Application-specific domain code belongs beside these re-exports.
pub mod schemas {
    pub use forklaunch_ecommerce_interface::{
        AddCartItemDto, AdjustStockDto, CartDto, CreateCartDto, CreateOrderDto, CreatePaymentDto, CreateProductDto,
        CreateVariantDto, InventoryDto, OrderDto, PaymentDto, ProductDto, RemoveCartItemDto, StockCheckDto,
        StockCheckResultDto, TransitionOrderDto, UpdateProductDto, UpdateVariantDto, VariantDto,
    };
}

pub mod services {
    pub use forklaunch_ecommerce_base::{BaseEcommerceService, events::OrderInventoryHandler, postgres::PostgresEcommerceStore};
    pub use forklaunch_ecommerce_interface::{
        CartService, InventoryService, OrderService, PaymentService, ProductService, VariantService,
    };
}
