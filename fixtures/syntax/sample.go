// Package sample exercises the Go grammar's highlight captures.
package sample

import (
	"context"
	"errors"
	"fmt"
	"strings"
)

/*
A block comment.

It spans a blank line.
*/

const (
	// Limit is exported.
	Limit   = 10
	ratio   = 0.75
	mask    = 0xff
	newline = '\n'
	raw     = `C:\path\with "quotes"`
)

var ErrEmpty = errors.New("empty: \"nothing\" here\t")

type Shape interface {
	Area() float64
	fmt.Stringer
}

type Point[T ~int | ~float64] struct {
	X, Y T `json:"x,omitempty"`
}

func (p Point[T]) Sum() T { return p.X + p.Y }

// Map applies f to each element.
func Map[S ~[]E, E, R any](items S, f func(E) R) []R {
	out := make([]R, 0, len(items))
	for _, item := range items {
		out = append(out, f(item))
	}
	return out
}

func run(ctx context.Context, words []string) (n int, err error) {
	defer func() {
		if r := recover(); r != nil {
			err = fmt.Errorf("recovered: %v", r)
		}
	}()
	done := make(chan struct{})
	go func() {
		defer close(done)
		n = len(strings.Join(words, " "))
	}()
outer:
	for i := 0; i < Limit; i++ {
		switch {
		case i%2 == 0:
			continue outer
		case i > 7:
			break outer
		default:
			_ = ratio * float64(i)
		}
	}
	select {
	case <-ctx.Done():
		return 0, ctx.Err()
	case <-done:
	}
	var shape Shape
	if s, ok := shape.(fmt.Stringer); ok && s != nil {
		fmt.Println(s.String(), true, false, nil, iota)
	}
	return n, nil
}

func main() {
	fmt.Println(Map([]int{1, 2, 3}, func(i int) string { return fmt.Sprint(i) }))
}
