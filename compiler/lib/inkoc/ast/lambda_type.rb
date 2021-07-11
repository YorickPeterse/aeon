# frozen_string_literal: true

module Inkoc
  module AST
    class LambdaType
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :arguments, :returns, :throws, :location
      attr_accessor :optional, :moving

      def initialize(args, returns, throws, location)
        @arguments = args
        @returns = returns
        @throws = throws
        @location = location
        @optional = false
        @moving = false
      end

      def optional?
        @optional
      end

      def visitor_method
        :on_lambda_type
      end

      def lambda_type?
        true
      end

      def lambda_or_block_type?
        true
      end

      def late_binding=(*)
        # Late binding does not apply to block types.
      end

      def late_binding?
        false
      end
    end
  end
end
