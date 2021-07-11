# frozen_string_literal: true

module Inkoc
  module AST
    class BlockType
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

      def late_binding=(*)
        # Late binding does not apply to block types.
      end

      def late_binding?
        false
      end

      def optional?
        @optional
      end

      def visitor_method
        :on_block_type
      end

      def block_type?
        true
      end

      def lambda_or_block_type?
        true
      end
    end
  end
end
