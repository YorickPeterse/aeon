# frozen_string_literal: true

module Inkoc
  module AST
    class Try
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :expression, :else_argument, :else_body, :location
      attr_accessor :else_argument_symbol

      def initialize(expr, else_body, else_arg, location)
        @expression = expr
        @else_argument = else_arg
        @else_body = else_body
        @location = location
        @else_argument_symbol = nil
      end

      def visitor_method
        :on_try
      end

      def explicit_block_for_else_body?
        else_argument || else_body.multiple_expressions?
      end

      def empty_else?
        else_body.empty?
      end

      def else_argument_name
        else_argument&.name
      end

      def throw_type
        compare_with = expression.cast? ? expression.expression : expression

        if compare_with.throw?
          compare_with.type
        elsif compare_with.send?
          compare_with.throw_type
        elsif compare_with.identifier?
          # The identifier might be a local variable, in which case "block_type"
          # is not set.
          compare_with.throw_type
        end
      end
    end
  end
end
