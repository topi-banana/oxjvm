package demo;

/**
 * Allocates and fills multi-dimensional arrays in a loop.
 *
 * This is the regression test for `multianewarray`: with a small heap the collector runs while
 * the outer array is still being filled, and an implementation that does not root the array it
 * is building loses it to the collector.
 */
public class MultiArray {
    public static int fill(int rounds) {
        int total = 0;
        for (int round = 0; round < rounds; round++) {
            int[][] grid = new int[96][96];
            for (int row = 0; row < grid.length; row++) {
                for (int column = 0; column < grid[row].length; column++) {
                    grid[row][column] = row + column;
                }
            }
            total += grid[95][95];
        }
        return total;
    }
}
