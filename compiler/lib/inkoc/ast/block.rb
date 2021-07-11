# frozen_string_literal: true

module Inkoc
  module AST
    class Block
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :arguments, :body, :throws, :returns, :location,
                  :type_parameters, :signature

      attr_accessor :type_scope, :moving

      # targs - The type arguments of this block.
      # arguments - The arguments of the block.
      # body - The body of the block as a Body node.
      # returns - The return type of the block.
      # throws - The type that may be thrown.
      # location - The SourceLocation of the block.
      # signature - Set to true when a signature was included.
      def initialize(targs, args, returns, throws, body, loc, signature: true)
        @type_parameters = targs
        @arguments = args
        @returns = returns
        @throws = throws
        @body = body
        @location = loc
        @signature = signature
        @lambda = false
        @type_scope = nil
        @moving = false
      end

      def infer_as_lambda
        @lambda = true

        body.locals.parent = nil
      end

      def visitor_method
        lambda? ? :on_lambda : :on_block
      end

      def block_type
        type
      end

      def closure?
        true
      end

      def block?
        true
      end

      def lambda?
        @lambda
      end

      def block_without_signature?
        !signature
      end

      def block_name
        Config::BLOCK_NAME
      end
    end
  end
end
