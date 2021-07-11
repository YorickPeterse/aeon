# frozen_string_literal: true

module Inkoc
  module AST
    class Lambda
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :arguments, :body, :throws, :returns, :location,
                  :type_parameters

      # targs - The type arguments of this block.
      # arguments - The arguments of the block.
      # body - The body of the block as a Body node.
      # returns - The return type of the block.
      # throws - The type that may be thrown.
      # location - The SourceLocation of the block.
      def initialize(targs, args, returns, throws, body, location)
        @type_parameters = targs
        @arguments = args
        @returns = returns
        @throws = throws
        @body = body
        @location = location
      end

      def visitor_method
        :on_lambda
      end

      def block_type
        type
      end

      def block?
        true
      end

      def lambda?
        true
      end

      def block_name
        Config::LAMBDA_NAME
      end
    end
  end
end
