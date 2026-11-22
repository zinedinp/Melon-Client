//! Recipes, the way the 26.3 client sees them (`ClientRecipeBook`).

pub mod book;
pub mod display;
pub mod layers;
pub mod sets;

pub use book::{
    BookSettings, BookType, Category, Collection, Entry, RecipeBook, RecipeId, Stock, Tab,
};
pub use display::{ItemSet, RecipeDisplay, SlotDisplay};
pub use sets::{PropertySet, RecipeSets};
