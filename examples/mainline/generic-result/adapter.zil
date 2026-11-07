import model;

pub fn forward<T:StoreElement>(give result:model.Error!Store<T>)->model.Error!Store<T>
contract { result(give) } => result

pub fn accept<T:StoreElement>(give pending:model.Error!Store<T>)->model.Error!Store<T>
contract { result(give) } {
    let moved=pending;
    return forward(moved)
}
