(defmacro cond clauses
  (if (= clauses ())
      ()
      (if (= (car (car clauses)) 'else)
          (car (cdr (car clauses)))
          `(if ,(car (car clauses))
               ,(car (cdr (car clauses)))
               (cond ,@(cdr clauses))))))
