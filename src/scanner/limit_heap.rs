use std::fmt::{Debug, Formatter, Result};

/// bespoke variation on an array-backed max-heap
///
/// 255 possible values generated from the initial requests/second
///
/// when no additional errors are encountered, the left child is taken (increasing req/sec)
/// if errors have increased since the last interval, the right child is taken (decreasing req/sec)
///
/// formula for each child:
/// - left: (|parent - current|) / 2 + current
/// - right: current - ((|parent - current|) / 2)
pub(super) struct LimitHeap {
    /// backing array, 255 nodes == height of 7 ( 2^(h+1) -1 nodes )
    pub(super) inner: [i32; 255],

    /// original # of requests / second
    pub(super) original: i32,

    /// current position w/in the backing array
    pub(super) current: usize,
}

/// default implementation of a LimitHeap
impl Default for LimitHeap {
    /// zero-initialize the backing array
    fn default() -> Self {
        Self {
            inner: [0; 255],
            original: 0,
            current: 0,
        }
    }
}

/// Debug implementation of a LimitHeap
impl Debug for LimitHeap {
    /// return debug representation that conforms to <32 elements in array
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let msg = format!(
            "LimitHeap {{ original: {}, current: {}, inner: [{}...] }}",
            self.original, self.current, self.inner[0]
        );
        write!(f, "{msg}")
    }
}

/// implementation of a LimitHeap
impl LimitHeap {
    /// move to right child, return node's index from which the move was requested
    pub(super) fn move_right(&mut self) -> usize {
        if self.has_children() {
            let tmp = self.current;
            self.current = self.current * 2 + 2;
            return tmp;
        }
        self.current
    }

    /// move to left child, return node's index from which the move was requested
    pub(super) fn move_left(&mut self) -> usize {
        if self.has_children() {
            let tmp = self.current;
            self.current = self.current * 2 + 1;
            return tmp;
        }
        self.current
    }

    /// move to parent, return node's index from which the move was requested
    pub(super) fn move_up(&mut self) -> usize {
        if self.has_parent() {
            let tmp = self.current;
            self.current = (self.current - 1) / 2;
            return tmp;
        }
        self.current
    }

    /// move directly to the given index
    pub(super) fn move_to(&mut self, index: usize) {
        self.current = index;
    }

    /// get the current node's value
    pub(super) fn value(&self) -> i32 {
        self.inner[self.current]
    }

    /// set the current node's value
    pub(super) fn set_value(&mut self, value: i32) {
        self.inner[self.current] = value;
    }

    /// check that this node has a parent (true for all except root)
    pub(super) fn has_parent(&self) -> bool {
        self.current > 0
    }

    /// get node's parent's value or self.original if at the root
    pub(super) fn parent_value(&mut self) -> i32 {
        if self.has_parent() {
            let current = self.move_up();
            let val = self.value();
            self.move_to(current);
            return val;
        }
        self.original
    }

    /// check if the current node has children
    pub(super) fn has_children(&self) -> bool {
        // inner structure is a complete tree, just check for the right child
        self.current * 2 + 2 <= self.inner.len()
    }

    /// get current node's right child's value
    fn right_child_value(&mut self) -> i32 {
        let tmp = self.move_right();
        let val = self.value();
        self.move_to(tmp);
        val
    }

    /// set current node's left child's value
    fn set_left_child(&mut self) {
        let parent = self.parent_value();
        let current = self.value();
        let value = ((parent - current).abs() / 2) + current;

        self.move_left();
        self.set_value(value);
        self.move_up();
    }

    /// set current node's right child's value
    fn set_right_child(&mut self) {
        let parent = self.parent_value();
        let current = self.value();
        let value = current - ((parent - current).abs() / 2);

        self.move_right();
        self.set_value(value);
        self.move_up();
    }

    /// iterate over the backing array, filling in each child's value based on the original value
    pub(super) fn build(&mut self) {
        // ex: original is 400
        // arr[0] == 200
        // arr[1] (left child) == 300
        // arr[2] (right child) == 100
        let root = self.original / 2;

        self.inner[0] = root; // set root node to half of the original value
        self.inner[1] = ((self.original - root).abs() / 2) + root;
        self.inner[2] = root - ((self.original - root).abs() / 2);

        // start with index 1 and fill in each child below that node
        for i in 1..self.inner.len() {
            self.move_to(i);

            if self.has_children() && self.right_child_value() == 0 {
                // this node has an unset child since the rchild is 0
                self.set_left_child();
                self.set_right_child();
            }
        }
        self.move_to(0); // reset current index to the root of the tree
    }
}
