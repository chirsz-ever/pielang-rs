#[test]
fn identify_pie_grammar() {
    let base = "\
#!/usr/bin/pie

;;; claim 与 define 测试

(claim n Nat)
(define n 893)

(claim foo (-> Nat Nat Nat))
(define foo (lambda (x y) x))

(claim ListNat U)
(define ListNat (List Nat))

;;; 表达式测试

0
1
9876

'a
'this-is-a-symbol

n

(lambda (x) x)

(the (= Nat 0 0) sole)

(Sigma ((x Nat)(y Nat)) (= Nat x y))

;;; 实际使用的代码

(claim + (-> Nat Nat Nat))
(define +
  (λ (n j)
    (iter-Nat n
      j
      (λ (s) (add1 s)))))

(claim dsub1 (-> Nat Nat))
(define dsub1 (λ (n)
               (which-Nat n
                 0
                 (λ (n-1) n-1))))

(claim -. (-> Nat Nat Nat))
(define -. (λ (m n)
             (rec-Nat n
               m
               (λ (n-1 t) (dsub1 t)))))

(claim mot-list->vec (Π ((E U)) (-> (List E) U)))
(define mot-list->vec (λ (E es) (Vec E (length E es))))

(claim list->vec (Π ((E U)(es (List E)))
                   (Vec E (length E es))))
(define list->vec (λ (E es)
                    (ind-List es
                      (mot-list->vec E)
                      vecnil
                      (λ (x xs v_p)
                        (vec:: x v_p)))))
";
    let leibniz = "\
#lang pie

(claim L-≡p (Π ((A U)) (→ A A (→ A U) U)))
(define L-≡p
  (λ (A x y P)
    (Pair (→ (P x) (P y)) (→ (P y) (P x)))))

; the real Leibniz Equality type is (Π ((P (→ A U))) (L-≡p A x y P)),
; but we can't create an alias for it

; for all x, x == x
(claim L-refl (Π ((A U)(x A)) (Π ((P (→ A U))) (L-≡p A x x P))))
(define L-refl
  (λ (A x P)
    (cons (λ (Px) Px) (λ (Py) Py))))

; x == y => P x -> P y
(claim L-subst
  (Π ((A U)(x A)(y A)(P (→ A U)))
    (→ (Π ((Q (→ A U))) (L-≡p A x y Q))
      (→ (P x) (P y)))))
(define L-subst
  (λ (A x y P x≡y) (car (x≡y P))))

; x == y and y == z => x == z
(claim L-trans
  (Π ((A U)(x A)(y A)(z A))
    (→ (Π ((P (→ A U))) (L-≡p A x y P)) (Π ((P (→ A U))) (L-≡p A y z P))
      (Π ((P (→ A U))) (L-≡p A x z P)))))
(define L-trans
  (λ (A x y z x≡y y≡z)
    (λ (P)
      (cons
        (λ (Px) ((car (y≡z P)) ((car (x≡y P)) Px)))
        (λ (Pz) ((cdr (x≡y P)) ((cdr (y≡z P)) Pz)))))))

; x == y => y == x
(claim L-sym
  (Π ((A U)(x A)(y A))
    (→ (Π ((P (→ A U))) (L-≡p A x y P))
      (Π ((P (→ A U))) (L-≡p A y x P)))))
(define L-sym
  (λ (A x y x≡y)
    (λ (P)
      (cons (cdr (x≡y P)) (car (x≡y P))))))

; x == y => f x == f y
(claim L-cong
  (Π ((A U)(B U)(f (→ A B))(x A)(y A))
    (→ (Π ((P (→ A U))) (L-≡p A x y P))
      (Π ((Q (→ B U))) (L-≡p B (f x) (f y) Q)))))
(define L-cong
  (λ (A B f x y x≡y)
    (λ (Q) (x≡y (λ (t) (Q (f t)))))))

; u == x and v == y => f u v == f x y
(claim L-cong-2
  (Π ((A U)(B U)(C U)(f (→ A B C))(u A)(x A)(v B)(y B))
    (→ (Π ((P (→ A U))) (L-≡p A u x P))
       (Π ((Q (→ B U))) (L-≡p B v y Q))
      (Π ((R (→ C U))) (L-≡p C (f u v) (f x y) R)))))
(define L-cong-2
  (λ (A B C f u x v y u≡x v≡y)
    (λ (R)
      (cons
        (λ (Rfuv)
          ((car (v≡y (λ (t) (R (f x t)))))
            ((car (u≡x (λ (t) (R (f t v))))) Rfuv)))
        (λ (Rfxy)
          ((cdr (v≡y (λ (t) (R (f u t)))))
            ((cdr (u≡x (λ (t) (R (f t y))))) Rfxy)))))))

; f == g => f x == g x
(claim L-cong-app
  (Π ((A U)(B U)(f (→ A B))(g (→ A B)))
    (→ (Π ((P (→ (→ A B) U))) (L-≡p (→ A B) f g P))
      (Π ((x A))
        (Π ((Q (→ B U))) (L-≡p B (f x) (g x) Q))))))
(define L-cong-app
  (λ (A B f g f≡g)
    (λ (x)
      (λ (Q) (f≡g (λ (t) (Q (t x))))))))
";
    let bracket = "(Pi ([x Nat][y Nat]) (= Nat x y))";
    let parser = crate::syntax::GrammerParser::new();
    parser.parse(base).expect("base test");
    parser.parse(leibniz).expect(r#"work code "Leibniz""#);
    parser.parse(bracket).expect("bracket test");
}

#[test]
fn parse_expression() {
    let parser = crate::syntax::ExprParser::new();
    let exprs = [
        // Nat literals
        "0",
        "1",
        "9876",
        "01",
        // FIXME: Pie 拒绝了 -1
        // "-1",

        // Atom literals
        "'a",
        "'-a",
        "'a-",
        "'atom",
        "'this-is-a-symbol",
        "'  btom",
        "(quote ctom)",
        "(quote this-is-a-symbol)",
        "(quote 'a)",
        // symbols
        "nil",
        // S-expressions
        "(the (List Nat) nil)",
        "(the(List Nat)nil)",
        "(cons 2 (same 2))",
        "(lambda (x) x)",
        "(λ (x) (add1 x))",
        r"(the (Σ ((n Nat))
         (= Nat n n))
    (cons 2 (same 2)))",
        // brackets and braces
        "[the Nat 1]",
        "{the Nat 1}",
    ];
    for e in exprs {
        insta::with_settings!({
            description => e,
        }, {
            insta::assert_debug_snapshot!(format!("parse_expression_{}", e), parser.parse(e),);
        });
    }
}
