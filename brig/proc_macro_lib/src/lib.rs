use {
    proc_macro::TokenStream,
    proc_macro2::{Ident, Span},
    quote::{ToTokens, quote},
    syn::{
        Abi, Attribute, ItemFn, LitStr, MetaList, Path, PathSegment, parse_macro_input,
        punctuated::Punctuated,
        token::{Bracket, Extern, Pound},
    },
};

/// Proc macro for generating IRQ/exception handlers
///
///  ```rust
/// #[irq_handler(with_code = true)]
/// fn timer() {
///    scheduler::schedule();
///
///    unsafe {
///        crate::devices::lapic::LAPIC
///            .get()
///            .unwrap()
///            .lock()
///            .inner
///            .end_of_interrupt()
///    };
/// }
/// ```
/// 
/// should generate
/// ```rust
///   #[naked]
///   unsafe extern "C" fn timer() -> ! {
///      #[no_mangle]
///     extern "C" fn timer_inner() {
///         scheduler::schedule();
///         unsafe {
///             crate::devices::lapic::LAPIC
///                 .get()
///                 .unwrap()
///                 .lock()
///                 .inner
///                 .end_of_interrupt()
///         };
///     }
///
///       core::arch::asm!(
///           concat!(
///               "
///   push $0
///   push %rax
///   push %rcx
///   push %rdx
///   push %rbx
///   push %rbp
///   push %rsi
///   push %rdi
///   push %r8
///   push %r9
///   push %r10
///   push %r11
///   push %r12
///   push %r13
///   push %r14
///   push %r15
///   mov %rsp, %gs:0
///   mov %rsp, %rdi
///   call timer_inner
///   mov %gs:0, %rsp
///   pop %r15
///   pop %r14
///   pop %r13
///   pop %r12
///   pop %r11
///   pop %r10
///   pop %r9
///   pop %r8
///   pop %rdi
///   pop %rsi
///   pop %rbp
///   pop %rbx
///   pop %rdx
///   pop %rcx
///   pop %rax
///   add $8, %rsp
///   iretq"
///           ),
///           options(att_syntax, noreturn)
///       );
///   }
/// ```
#[proc_macro_attribute]
pub fn irq_handler(args: TokenStream, input: TokenStream) -> TokenStream {
    // todo: parse this better
    let with_code = {
        let args = args.into_iter().collect::<Vec<_>>();

        if args[0].to_string() != "with_code" || args[1].to_string() != "=" {
            panic!("argument must be `with_code = true` or `with_code = false`");
        }

        match args[2].to_string().as_str() {
            "true" => true,
            "false" => false,
            other => {
                panic!("`with_code` must be `true` or `false`, got {other:?}");
            }
        }
    };

    let mut inner_fn = parse_macro_input!(input as ItemFn);

    let outer_fn_ident = inner_fn.sig.ident;
    let inner_fn_ident = outer_fn_ident.clone().to_string() + "_inner";

    // input is the inner function
    // must be renamed to _inner
    inner_fn.sig.ident = syn::Ident::new(&inner_fn_ident, proc_macro2::Span::call_site());

    // Add `#[no_mangle]` attribute
    inner_fn.attrs.push(Attribute {
        pound_token: Pound {
            spans: [Span::call_site(); 1],
        },
        style: syn::AttrStyle::Outer,
        bracket_token: Bracket::default(),
        meta: syn::Meta::List(MetaList {
            path: Path {
                leading_colon: None,
                segments: Punctuated::from_iter([PathSegment {
                    ident: Ident::new("unsafe", Span::call_site()),
                    arguments: syn::PathArguments::None,
                }]),
            },
            delimiter: syn::MacroDelimiter::Paren(syn::token::Paren(Span::call_site())),
            tokens: quote!(no_mangle),
        }),
    });

    // Make function `extern "C"`
    inner_fn.sig.abi = Some(Abi {
        extern_token: Extern::default(),
        name: Some(LitStr::new("C", Span::call_site())),
    });

    let asm = generate_asm(&inner_fn_ident, with_code);

    // new outer function created with original function name
    let outer_fn = quote!(
        #[unsafe(naked)]
        unsafe extern "C" fn #outer_fn_ident() {
            // original user-supplied function definition
            #inner_fn

            core::arch::naked_asm!(#asm, options(att_syntax));
        }
    );

    TokenStream::from(outer_fn.to_token_stream())
}

fn generate_asm(inner_fn_ident: &str, with_code: bool) -> String {
    format!(
        "{}
   push %rax
   push %rcx
   push %rdx
   push %rbx
   push %rbp
   push %rsi
   push %rdi
   push %r8
   push %r9
   push %r10
   push %r11
   push %r12
   push %r13
   push %r14
   push %r15

   push %gs:0
   mov %rsp, %gs:0

   mov %rsp, %rdi
   call {inner_fn_ident}
   mov %gs:0, %rsp
   pop %gs:0

   pop %r15
   pop %r14
   pop %r13
   pop %r12
   pop %r11
   pop %r10
   pop %r9
   pop %r8
   pop %rdi
   pop %rsi
   pop %rbp
   pop %rbx
   pop %rdx
   pop %rcx
   pop %rax
   add $8, %rsp
   iretq",
        if !with_code { "push $0" } else { "" }
    )
}

#[proc_macro_attribute]
pub fn ktest(_attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item: ItemFn = parse_macro_input!(item);
    let fn_name = item.sig.ident.clone();
    let fn_name_str = fn_name.to_string();
    let static_name = Ident::new(
        &format!("TEST_{}", fn_name_str.to_ascii_uppercase()),
        Span::call_site(),
    );

    quote! {
        #[linkme::distributed_slice(crate::tests::TESTS)]
        static #static_name: (&'static str, fn()) = (#fn_name_str, #fn_name);

        #item
    }
    .into()
}

#[proc_macro_attribute]
pub fn guest_device_factory(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let item: ItemFn = parse_macro_input!(item);
    let fn_name = item.sig.ident.clone();
    let proc_macro::TokenTree::Ident(device_kind) = attribute.into_iter().next().unwrap() else {
        panic!("missing device kind");
    };

    let device_kind = device_kind.to_string();

    let static_name = Ident::new(
        &format!("GDF_{}", device_kind.to_ascii_uppercase()),
        Span::call_site(),
    );

    quote! {
        #[linkme::distributed_slice(crate::guest::devices::DEVICE_FACTORIES)]
        static #static_name: (&'static str, fn(&BTreeMap<InternedString, InternedString>) -> Arc<dyn Device>) = (#device_kind, |config| {
            let device = #fn_name(config);
            ObjectStore::global().insert(device.clone());
            device
        });

        #item
    }
    .into()
}
